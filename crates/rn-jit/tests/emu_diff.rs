//! The micro-op IR run by the Rust emulator (Miri-runnable) and by the x86-64 backend on identical memory:
//! same exit code, same memory image. Templates are driven with hand-assembled bytecode on synthetic
//! frame / `CallInfo` / thread buffers (no Lua state needed).
#![cfg(feature = "jit")]

use rn_core::instruction::Instruction;
use rn_core::jit_hooks::thread as thr;
use rn_core::jit_layout::callinfo as ci;
use rn_core::value::LUA_VNUMINT;
use rn_core::ver::active::OpCode;
use rn_jit::emu::{self, Env, Stop};
use rn_jit::lower::{lower, LowerCfg, Lowered};

const FAKE_CODE: usize = 0x10_0000;
const NSLOT: usize = 40;
const PRE: usize = 2; // slots below the frame base (the function slot is the first one above them)

/// All memory the program touches.
#[derive(Clone)]
struct Mem {
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    cnt: i32,
    irq: Vec<u8>, // (heap: a pointer to it survives later `&mut Mem` reborrows)
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
        self.frame[(PRE + r) * 2] = u64::from(tag);
        self.frame[(PRE + r) * 2 + 1] = payload;
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

fn lowered(code: &[Instruction], irq: *const u8) -> Lowered {
    let words: Vec<u32> = code.iter().map(|i| i.0).collect();
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

fn run_emu(low: &Lowered, m: &mut Mem, start_pc: u32) -> Stop {
    let (_, l) = *low
        .resume
        .iter()
        .find(|(pc, _)| *pc == start_pc)
        .expect("resume entry");
    let env = m.env();
    // SAFETY: the buffers in `m` are large enough for every offset the templates use.
    unsafe { emu::run(&low.prog, l, &env, 10_000_000) }
}

/// Register of the control variable of a numeric `for` at register 0.
const CTL: usize = if rn_core::jit_layout::LUA54 { 3 } else { 2 };

/// `for r0 = init, limit, step do <body of one MOVE> end; return`-shaped function.
fn for_loop(ret: OpCode) -> Vec<Instruction> {
    vec![
        Instruction::abx(OpCode::Forprep, 0, 1), // 0: skip to 3 + 0... (target pc+bx+2 = 3)
        // 1: body (copies the control variable: `R[A+2]` in 5.5, `R[A+3]` in 5.4)
        Instruction::abck(OpCode::Move, 4, if CTL == 3 { 3 } else { 2 }, 0, 0),
        Instruction::abx(OpCode::Forloop, 0, 2), // 2: back to 1
        Instruction::abck(ret, 4, 2, 0, 0),      // 3: return
    ]
}

#[test]
fn emulator_runs_a_counted_loop() {
    let code = for_loop(OpCode::Return1);
    let mut m = Mem::new();
    let irq = m.irq.as_ptr();
    let low = lowered(&code, irq);
    m.setup_ci(2, 0); // one result wanted
    m.set_reg(0, LUA_VNUMINT, 1);
    m.set_reg(1, LUA_VNUMINT, 5);
    m.set_reg(2, LUA_VNUMINT, 1);
    let stop = run_emu(&low, &mut m, 0);
    assert_eq!(stop, Stop::Ret(1)); // FrameChanged
    assert_eq!(m.frame[(PRE + CTL) * 2 + 1], 5, "control variable");
    assert_eq!(
        m.frame[(PRE + 4) * 2 + 1],
        5,
        "body ran with the last value"
    );
    // RETURN1 moved r4 to the function slot (the slot below the base)
    assert_eq!(m.frame[(PRE - 1) * 2 + 1], 5);
    // SAFETY: the thread buffer holds the written `top` at `thr::TOP`.
    let top: usize = unsafe {
        m.thread
            .as_ptr()
            .cast::<u8>()
            .add(thr::TOP)
            .cast::<usize>()
            .read_unaligned()
    };
    assert_eq!(top, 6); // func (5) + 1
}

#[test]
fn emulator_honours_the_count_budget() {
    let code = for_loop(OpCode::Return0);
    let mut m = Mem::new();
    let low = lowered(&code, m.irq.as_ptr());
    m.setup_ci(1, 0);
    m.set_reg(0, LUA_VNUMINT, 1);
    m.set_reg(1, LUA_VNUMINT, 1000);
    m.set_reg(2, LUA_VNUMINT, 1);
    m.cnt = 100;
    let stop = run_emu(&low, &mut m, 0);
    assert_eq!(stop, Stop::Ret(0), "budget exhausted -> Continue");
    let pc: usize = {
        // SAFETY: savedpc was written at `ci::SAVEDPC`.
        unsafe {
            m.ci.as_ptr()
                .cast::<u8>()
                .add(ci::SAVEDPC)
                .cast::<usize>()
                .read_unaligned()
        }
    };
    assert!((FAKE_CODE..FAKE_CODE + 16).contains(&pc), "{pc:#x}");
}

/// `r2 = r0 + r1; return r2` (the MMBIN follower is skipped on the fast path).
fn add_function() -> Vec<Instruction> {
    vec![
        Instruction::abck(OpCode::Add, 2, 0, 1, 0),
        Instruction::abck(OpCode::Mmbin, 0, 1, 6, 0),
        Instruction::abck(OpCode::Return1, 2, 2, 0, 0),
        Instruction::abck(OpCode::Return0, 0, 1, 0, 0),
    ]
}

#[test]
fn emulator_adds_ints_floats_and_mixed_and_exits_on_the_rest() {
    use rn_core::value::{LUA_VNIL, LUA_VNUMFLT};
    let code = add_function();
    type Raw = (u8, u64);
    let cases: [(Raw, Raw, Option<Raw>); 6] = [
        ((LUA_VNUMINT, 2), (LUA_VNUMINT, 3), Some((LUA_VNUMINT, 5))),
        (
            (LUA_VNUMINT, i64::MAX as u64),
            (LUA_VNUMINT, 1),
            Some((LUA_VNUMINT, i64::MIN as u64)),
        ),
        (
            (LUA_VNUMFLT, 1.5f64.to_bits()),
            (LUA_VNUMFLT, 2.25f64.to_bits()),
            Some((LUA_VNUMFLT, 3.75f64.to_bits())),
        ),
        (
            (LUA_VNUMINT, 2),
            (LUA_VNUMFLT, 0.5f64.to_bits()),
            Some((LUA_VNUMFLT, 2.5f64.to_bits())),
        ),
        (
            (LUA_VNUMFLT, 0.5f64.to_bits()),
            (LUA_VNUMINT, 3),
            Some((LUA_VNUMFLT, 3.5f64.to_bits())),
        ),
        ((LUA_VNIL, 0), (LUA_VNUMINT, 1), None),
    ];
    for (a, b, want) in cases {
        let mut m = Mem::new();
        let low = lowered(&code, m.irq.as_ptr());
        m.setup_ci(2, 0);
        m.set_reg(0, a.0, a.1);
        m.set_reg(1, b.0, b.1);
        let stop = run_emu(&low, &mut m, 0);
        match want {
            Some(w) => {
                assert_eq!(stop, Stop::Ret(1), "{a:?} + {b:?}");
                assert_eq!(
                    (m.frame[(PRE - 1) * 2] as u8, m.frame[(PRE - 1) * 2 + 1]),
                    w
                );
            }
            None => {
                assert_eq!(stop, Stop::Ret(0), "{a:?} + {b:?} must exit");
                // the exit is at the ADD itself and the arguments are untouched
                assert_eq!(m.frame[PRE * 2 + 1], a.1);
            }
        }
    }
}

/// C1: a block pre-pays its templates; an exit refunds the instruction that exits and the rest of the block, so
/// the budget drops by exactly the number of instructions that completed in compiled code.
#[test]
fn exits_refund_the_prepaid_count() {
    use rn_core::value::LUA_VNIL;
    let code = vec![
        Instruction::abck(OpCode::Add, 2, 0, 1, 0),
        Instruction::abck(OpCode::Mmbin, 0, 1, 6, 0),
        Instruction::abck(OpCode::Add, 3, 4, 4, 0),
        Instruction::abck(OpCode::Mmbin, 4, 4, 6, 0),
        Instruction::abck(OpCode::Return1, 3, 2, 0, 0),
        Instruction::abck(OpCode::Return0, 0, 1, 0, 0),
    ];
    for (r4, stop, used) in [
        ((LUA_VNIL, 0u64), Stop::Ret(0), 1), // the second ADD exits: only the first completed
        ((LUA_VNUMINT, 4), Stop::Ret(1), 3), // two ADDs and the RETURN1
    ] {
        let mut m = Mem::new();
        let low = lowered(&code, m.irq.as_ptr());
        m.setup_ci(2, 0);
        m.cnt = 100;
        m.set_reg(0, LUA_VNUMINT, 1);
        m.set_reg(1, LUA_VNUMINT, 2);
        m.set_reg(4, r4.0, r4.1);
        assert_eq!(run_emu(&low, &mut m, 0), stop);
        assert_eq!(100 - m.cnt, used, "instructions counted");
    }
}

// ---- native vs emulator -------------------------------------------------------------------------------

#[cfg(not(miri))]
mod native {
    use super::*;
    use rn_jit::mem::CodeArena;
    use rn_jit::x86::{emit, thunk_code, Thunk};

    pub fn run_native(low: &Lowered, m: &mut Mem, start_pc: u32) -> u32 {
        let em = emit(&low.prog).expect("assembles");
        let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == start_pc).unwrap();
        let off = em.label_off[l as usize].expect("bound");
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
        // SAFETY: the buffers in `m` are large enough for every offset the templates use; the target is a
        // resume label of the committed block.
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
}

#[cfg(not(miri))]
#[test]
fn native_matches_emulator_on_random_loops_and_returns() {
    use rn_core::value::{LUA_VNIL, LUA_VNUMFLT};
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let interesting = [
        0i64,
        1,
        -1,
        2,
        -2,
        3,
        7,
        100,
        -100,
        i64::MAX,
        i64::MAX - 1,
        i64::MIN,
        i64::MIN + 1,
        1 << 40,
    ];
    for case in 0..600 {
        let ret = if rnd() & 1 == 0 {
            OpCode::Return0
        } else {
            OpCode::Return1
        };
        let code = for_loop(ret);
        let pick = |r: u64, rnd: &mut dyn FnMut() -> u64| -> i64 {
            match r % 3 {
                0 => interesting[(rnd() % interesting.len() as u64) as usize],
                1 => (rnd() % 21) as i64 - 10,
                _ => rnd() as i64,
            }
        };
        let (init, limit, mut step) = (
            pick(rnd(), &mut rnd),
            pick(rnd(), &mut rnd),
            pick(rnd(), &mut rnd),
        );
        if step == 0 {
            step = 1;
        }
        // keep the iteration count small so that the test is quick
        let count_hint = (limit as i128 - init as i128) / (step as i128);
        let limit = if count_hint.unsigned_abs() > 50 {
            (init as i128 + step as i128 * (rnd() % 20) as i128)
                .clamp(i64::MIN as i128, i64::MAX as i128) as i64
        } else {
            limit
        };
        let nres = (rnd() % 5) as u32; // callstatus low byte = nresults + 1; 0 = multret
        let fresh = if rnd() & 1 == 0 { ci::CIST_FRESH } else { 0 };
        let mmret = if rnd() % 8 == 0 { ci::CIST_MMRET } else { 0 };
        let trap = i32::from(rnd() % 8 == 0);
        let float_loop = rnd() % 12 == 0;
        let budget = if rnd() % 5 == 0 {
            (rnd() % 12) as i32
        } else {
            i32::MAX
        };

        let mut m = Mem::new();
        m.setup_ci(nres | fresh | mmret, trap);
        m.cnt = budget;
        if float_loop {
            // float loops run natively since J1b: keep the trip count small
            m.set_reg(0, LUA_VNUMFLT, 1.0f64.to_bits());
            m.set_reg(1, LUA_VNUMINT, rnd() % 20);
            step = (rnd() % 5) as i64 + 1;
        } else {
            m.set_reg(0, LUA_VNUMINT, init as u64);
            m.set_reg(1, LUA_VNUMINT, limit as u64);
        }
        m.set_reg(2, LUA_VNUMINT, step as u64);
        m.set_reg(3, LUA_VNIL, 0);
        m.set_reg(4, LUA_VNUMINT, 0xdead);
        m.irq[0] = u8::from(rnd() % 16 == 0);
        let mut m_emu = m.clone();
        let mut m_nat = m.clone();
        let irq_e = m_emu.irq.as_ptr();
        // the interrupt flag lives at a different address in each clone: lower per clone
        let low_e = lowered(&code, irq_e);
        let low_n = lowered(&code, m_nat.irq.as_ptr());
        let stop = run_emu(&low_e, &mut m_emu, 0);
        let code_n = native::run_native(&low_n, &mut m_nat, 0);
        let Stop::Ret(code_e) = stop else {
            panic!("emulator did not finish (case {case})");
        };
        assert_eq!(
            code_e, code_n,
            "exit code, case {case}: {init} {limit} {step}"
        );
        assert_eq!(m_emu.frame, m_nat.frame, "frame, case {case}");
        assert_eq!(m_emu.thread, m_nat.thread, "thread, case {case}");
        assert_eq!(m_emu.cnt, m_nat.cnt, "budget, case {case}");
        // savedpc and trap are the only CallInfo fields compiled code writes
        let rd = |m: &Mem, off: usize| -> u64 {
            // SAFETY: reads inside the ci buffer.
            unsafe {
                m.ci.as_ptr()
                    .cast::<u8>()
                    .add(off)
                    .cast::<u64>()
                    .read_unaligned()
            }
        };
        assert_eq!(
            rd(&m_emu, ci::TRAP) as u32,
            rd(&m_nat, ci::TRAP) as u32,
            "trap, case {case}"
        );
        assert_eq!(
            rd(&m_emu, ci::SAVEDPC),
            rd(&m_nat, ci::SAVEDPC),
            "savedpc, case {case}"
        );
    }
}

/// Native-only ops of the call protocol (`CallH`, `ReloadFrame`, `Shl`): the helper gets the pinned thread and
/// `CallInfo` and the two immediates, its 32-bit result lands in V0, `ReloadFrame` re-derives `rbx` from the
/// thread's stack pointer and `ci.func`, and the emulator refuses the native-only ops.
#[cfg(not(miri))]
#[test]
fn call_ops_run_natively() {
    use rn_jit::ir::{Base, Op, Program, Src, M, W};
    use std::sync::Mutex;
    static SEEN: Mutex<Option<(usize, usize, u64, u64)>> = Mutex::new(None);
    extern "C" fn helper(th: *mut u8, c: *mut u8, a: u64, b: u64) -> u32 {
        *SEEN.lock().unwrap() = Some((th as usize, c as usize, a, b));
        (a * 100 + b) as u32
    }
    let mut p = Program::default();
    let l0 = p.label();
    let frame = |off| M {
        base: Base::Frame,
        off,
    };
    for op in [
        Op::Bind(l0),
        Op::Mov(1, 3),
        Op::Shl(1, 4),
        Op::St {
            w: W::B8,
            m: frame(0),
            src: Src::V(1),
        },
        Op::CallH {
            f: helper as *const () as usize as u64,
            a: 7,
            b: 9,
        },
        Op::St {
            w: W::B8,
            m: frame(8),
            src: Src::V(0),
        },
        Op::ReloadFrame { stack_off: 0x40 },
        Op::St {
            w: W::B8,
            m: frame(0),
            src: Src::Imm(0x55),
        },
        Op::Ret(1),
    ] {
        p.push(op);
    }
    let low = Lowered {
        points: Vec::new(),
        prog: p,
        resume: vec![(0, l0)],
        resume_mm: Vec::new(),
        templates: 0,
        entry_ok: true,
        interp_entry: true,
        total: 0,
        loop_total: 0,
        loop_templates: 0,
        pc_labels: Vec::new(),
        land_labels: Vec::new(),
        sig: Default::default(),
        backedges: Vec::new(),
    };
    let mut m = Mem::new();
    m.setup_ci(1, 0);
    Mem::put(&mut m.ci, ci::FUNC, 2usize); // frame base = stack + (2 + 1) * 16
    let stack = m.frame.as_mut_ptr() as usize;
    Mem::put(&mut m.thread, 0x40, stack);
    let env = m.env();
    let code = native::run_native(&low, &mut m, 0);
    assert_eq!(code, 1);
    assert_eq!(m.frame[PRE * 2], 48, "Shl");
    assert_eq!(m.frame[PRE * 2 + 1], 709, "helper result in V0");
    assert_eq!(
        m.frame[3 * 2],
        0x55,
        "ReloadFrame: rbx = stack + (func + 1) * 16"
    );
    assert_eq!(
        *SEEN.lock().unwrap(),
        Some((env.thread as usize, env.ci as usize, 7, 9))
    );
    // the emulator reports the native-only ops instead of guessing
    let mut m2 = Mem::new();
    let env2 = m2.env();
    // SAFETY: the program only touches the (valid) frame buffer before the unsupported op.
    let stop = unsafe { emu::run(&low.prog, l0, &env2, 1000) };
    assert_eq!(stop, Stop::Unsupported);
}
