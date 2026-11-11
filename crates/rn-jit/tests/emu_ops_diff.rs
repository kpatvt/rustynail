//! Tier 1.5 micro-ops (docs/JIT_TIER15.md 4.2, task T1): XMM file, memory-operand compares, indexed addressing,
//! `F2IExact`, `run_until`, and the V7/V8 clobbers of the old ops.
//!
//! * Known-answer tests run the emulator only (Miri-clean).
//! * The differential tests build ONE program per case (prologue loads all V and X registers from memory, the op
//!   under test, an epilogue stores every register and sets a marker for branches), compile it once and run it on
//!   hundreds of random states, natively and in the emulator on identical memory. Registers and memory must match
//!   bit for bit (pointers are normalised: native and emulator run on different buffers). Each op gets more than
//!   10 000 states.
#![cfg(feature = "jit")]

use rn_core::jit_layout::callinfo as ci;
use rn_jit::emu::{self, Env, Stop, Until};
use rn_jit::ir::*;

// ---- frame layout (u64 words) --------------------------------------------------------------------------------

const VIN: usize = 0;
const XIN: usize = 16;
const VOUT: usize = 32;
const XOUT: usize = 48;
const MARK: usize = 64;
const DATA: usize = 128; // 96 words = 768 bytes of random data
const DATA_BYTES: usize = 768;
const PIVOT: usize = 1280; // byte offset handed to pointer registers (the middle of the data area)
const NWORDS: usize = 256;

#[derive(Clone)]
struct Buf {
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    cnt: i32,
}

impl Buf {
    fn new() -> Buf {
        Buf {
            frame: vec![0; NWORDS],
            ci: vec![0; ci::SIZE.div_ceil(8) + 8],
            thread: vec![0; 64],
            k: vec![0; 8],
            cnt: i32::MAX,
        }
    }
    fn base(&self) -> u64 {
        self.frame.as_ptr() as usize as u64
    }
    fn env(&mut self) -> Env {
        Env {
            frame: self.frame.as_mut_ptr().cast(),
            ci: self.ci.as_mut_ptr().cast(),
            thread: self.thread.as_mut_ptr().cast(),
            k: self.k.as_ptr().cast(),
            cnt: &raw mut self.cnt,
        }
    }
    /// Frame words with pointers into the frame replaced by their offsets.
    fn normalised(&self) -> Vec<u64> {
        let b = self.base();
        self.frame
            .iter()
            .map(|&w| {
                let d = w.wrapping_sub(b);
                if d < 4096 {
                    0xADD0_0000_0000_0000 | d
                } else {
                    w
                }
            })
            .collect()
    }
}

fn fr(off: usize) -> M {
    M {
        base: Base::Frame,
        off: off as i32,
    }
}

fn prologue(p: &mut Program, l0: L) {
    p.push(Op::Bind(l0));
    for i in 0..NV {
        p.push(Op::Ld {
            dst: i as V,
            w: W::B8,
            m: fr(8 * (VIN + i)),
        });
    }
    for i in 0..NX {
        p.push(Op::LdF {
            dst: i as X,
            m: fr(8 * (XIN + i)),
        });
    }
}

fn epilogue(p: &mut Program) {
    for i in 0..NX {
        p.push(Op::StF {
            m: fr(8 * (XOUT + i)),
            src: i as X,
        });
    }
    for i in 0..NV {
        p.push(Op::St {
            w: W::B8,
            m: fr(8 * (VOUT + i)),
            src: Src::V(i as V),
        });
    }
    p.push(Op::Ret(0));
}

/// A whole program: prologue, `body`, epilogue. `body` may allocate labels on the program it receives.
fn program(body: impl FnOnce(&mut Program) -> Vec<Op>) -> (Program, L) {
    let mut p = Program::default();
    let l0 = p.label();
    prologue(&mut p, l0);
    for op in body(&mut p) {
        p.push(op);
    }
    epilogue(&mut p);
    (p, l0)
}

/// `[branch op targeting t; MARK = 1; goto e; t: MARK = 2; e:]`
fn branchy(p: &mut Program, mk: impl FnOnce(L) -> Op) -> Vec<Op> {
    let (t, e) = (p.label(), p.label());
    let mark = |v| Op::St {
        w: W::B8,
        m: fr(8 * MARK),
        src: Src::Imm(v),
    };
    vec![
        mk(t),
        mark(1),
        Op::Jmp(e),
        Op::Bind(t),
        mark(2),
        Op::Bind(e),
    ]
}

// ---- random states --------------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

const SPECIAL_F: [u64; 24] = [
    0x0000_0000_0000_0000, // +0
    0x8000_0000_0000_0000, // -0
    0x3FF0_0000_0000_0000, // 1
    0xBFF0_0000_0000_0000, // -1
    0x7FF0_0000_0000_0000, // +inf
    0xFFF0_0000_0000_0000, // -inf
    0x7FF8_0000_0000_0000, // qnan
    0xFFF8_0000_0000_0000, // default nan
    0x7FF0_0000_0000_0001, // snan
    0xFFF0_0000_DEAD_BEEF, // snan, negative, payload
    0x7FF8_0000_1234_5678, // qnan payload
    0x7FFF_FFFF_FFFF_FFFF, // max payload nan
    0x0000_0000_0000_0001, // min denormal
    0x000F_FFFF_FFFF_FFFF, // max denormal
    0x0010_0000_0000_0000, // min normal
    0x7FEF_FFFF_FFFF_FFFF, // max
    0xFFEF_FFFF_FFFF_FFFF, // -max
    0x43E0_0000_0000_0000, // 2^63
    0xC3E0_0000_0000_0000, // -2^63
    0x43DF_FFFF_FFFF_FFFF, // largest below 2^63
    0xC3E0_0000_0000_0001, // just below -2^63
    0x4340_0000_0000_0000, // 2^53
    0x3FE0_0000_0000_0000, // 0.5
    0xBFE0_0000_0000_0000, // -0.5
];

fn rnd_f(rng: &mut Rng) -> u64 {
    match rng.below(9) {
        0 => rng.next(),
        1 | 2 => SPECIAL_F[rng.below(SPECIAL_F.len())],
        3 => {
            // NaN with random sign, quiet bit and payload
            let m = (rng.next() & 0x000F_FFFF_FFFF_FFFF) | 1;
            (rng.next() & 0x8000_0000_0000_0000) | 0x7FF0_0000_0000_0000 | m
        }
        4 => (rng.next() & 0x8000_0000_0000_0000) | (rng.next() & 0x000F_FFFF_FFFF_FFFF), // zero/denormal
        5 => {
            let v = (rng.below(2001) as i64 - 1000) as f64;
            if v == 0.0 && rng.chance(2) {
                (-0.0f64).to_bits()
            } else {
                v.to_bits()
            }
        }
        6 => (rnd_i(rng) as f64).to_bits(),
        7 => {
            // around +-2^63
            let base = if rng.chance(2) {
                0x43E0_0000_0000_0000u64
            } else {
                0xC3E0_0000_0000_0000
            };
            base.wrapping_add(rng.below(5) as u64).wrapping_sub(2)
        }
        _ => {
            // ordinary numbers of moderate magnitude (finite results of + - * /)
            let e = 0x3FF + rng.below(40) as u64 - 20;
            (rng.next() & 0x8000_0000_0000_0000) | (e << 52) | (rng.next() & 0x000F_FFFF_FFFF_FFFF)
        }
    }
}

const SPECIAL_I: [i64; 14] = [
    0,
    1,
    -1,
    2,
    -2,
    3,
    7,
    255,
    256,
    i64::MAX,
    i64::MIN,
    i64::MAX - 1,
    i64::MIN + 1,
    1 << 40,
];

fn rnd_i(rng: &mut Rng) -> i64 {
    match rng.below(4) {
        0 => SPECIAL_I[rng.below(SPECIAL_I.len())],
        1 => rng.below(41) as i64 - 20,
        2 => (rng.next() as i64) >> rng.below(64),
        _ => rng.next() as i64,
    }
}

#[derive(Clone, Debug)]
struct State {
    vin: [u64; NV],
    xin: [u64; NX],
    data: Vec<u8>,
}

impl State {
    fn random(rng: &mut Rng) -> State {
        let mut st = State {
            vin: [0; NV],
            xin: [0; NX],
            data: Vec::with_capacity(DATA_BYTES),
        };
        for v in &mut st.vin {
            *v = rnd_i(rng) as u64;
        }
        for x in &mut st.xin {
            *x = rnd_f(rng);
        }
        // equal operands and sign flips (min/max/compare of equal values, +0 vs -0)
        if rng.chance(5) {
            let (a, b) = (rng.below(NX), rng.below(NX));
            st.xin[a] = st.xin[b];
            if rng.chance(2) {
                st.xin[a] ^= 0x8000_0000_0000_0000;
            }
        }
        while st.data.len() < DATA_BYTES {
            let w = if rng.chance(2) {
                rnd_f(rng)
            } else {
                rnd_i(rng) as u64
            };
            st.data.extend_from_slice(&w.to_le_bytes());
        }
        st
    }
    /// Write `bytes` into the data area at frame byte offset `off`.
    fn patch(&mut self, off: usize, bytes: &[u8]) {
        let o = off - DATA * 8;
        self.data[o..o + bytes.len()].copy_from_slice(bytes);
    }
    fn apply(&self, b: &mut Buf, ptr_regs: &[V]) {
        let base = b.base();
        for i in 0..NV {
            b.frame[VIN + i] = self.vin[i];
        }
        for &r in ptr_regs {
            b.frame[VIN + r as usize] = base + PIVOT as u64;
        }
        b.frame[XIN..XIN + NX].copy_from_slice(&self.xin);
        // SAFETY: DATA*8 + DATA_BYTES <= NWORDS*8.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.data.as_ptr(),
                b.frame.as_mut_ptr().cast::<u8>().add(DATA * 8),
                DATA_BYTES,
            );
        }
    }
}

type Tweak = Box<dyn Fn(&mut State, &mut Rng)>;
type Model = Box<dyn Fn(&State, &[u64])>;

/// One generated test program with its per-state tweaks and an independent model check.
struct Case {
    prog: Program,
    l0: L,
    /// Registers that hold `frame + PIVOT`.
    ptr_regs: Vec<V>,
    /// Adjust a random state (set an index register, make a compared memory word equal, ...).
    tweak: Tweak,
    /// Output V registers not compared (poisoned by the emulator: `V7`/`V8` after division).
    skip_v: Vec<usize>,
    /// Check the final memory against a model of the op written in plain Rust (input state, output frame words).
    model: Model,
}

impl Case {
    fn new(prog: Program, l0: L) -> Case {
        Case {
            prog,
            l0,
            ptr_regs: Vec::new(),
            tweak: Box::new(|_, _| {}),
            skip_v: Vec::new(),
            model: Box::new(|_, _| {}),
        }
    }
}

fn run_emu(c: &Case, b: &mut Buf) -> Stop {
    let env = b.env();
    // SAFETY: the buffers are large enough for every offset the programs use (see the layout constants).
    unsafe { emu::run(&c.prog, c.l0, &env, 100_000) }
}

#[cfg(not(miri))]
mod native {
    use super::*;
    use rn_jit::mem::{CodeArena, CodeBlock};
    use rn_jit::x86::{emit, thunk_code, Thunk};

    pub struct Native {
        _arena: CodeArena,
        _blocks: (CodeBlock, CodeBlock),
        thunk: Thunk,
        addr: usize,
    }

    impl Native {
        pub fn new(prog: &Program, l0: L) -> Native {
            let em = emit(prog).expect("assembles");
            let off = em.label_off[l0 as usize].expect("bound");
            let mut arena = CodeArena::new();
            let tc = thunk_code();
            let mut tb = arena.alloc(tc.len(), 16).unwrap();
            arena.writer(&mut tb).unwrap().write_at(0, &tc);
            let mut cb = arena.alloc(em.code.len(), 16).unwrap();
            arena.writer(&mut cb).unwrap().write_at(0, &em.code);
            arena.commit().unwrap();
            // SAFETY: `tb` holds the thunk (committed); `Thunk` is its signature.
            let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
            let addr = cb.addr() + off;
            Native {
                _arena: arena,
                _blocks: (tb, cb),
                thunk,
                addr,
            }
        }
        pub fn run(&self, b: &mut Buf) -> u32 {
            let env = b.env();
            // SAFETY: the buffers are large enough for every offset the programs use; the target is the start
            // label of the committed block.
            unsafe {
                (self.thunk)(
                    env.thread,
                    env.ci,
                    env.frame,
                    env.cnt,
                    self.addr as *const u8,
                )
            }
        }
    }
}

/// Run `ncases` generated programs on `nstates` random states each, natively and in the emulator.
#[cfg(not(miri))]
fn drive(name: &str, ncases: usize, nstates: usize, gen: impl Fn(&mut Rng) -> Case) {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for b in name.bytes() {
        seed = seed.rotate_left(5) ^ u64::from(b);
        seed = seed.wrapping_mul(0x1000_0000_01B3);
    }
    let mut rng = Rng(seed | 1);
    let mut total = 0;
    for ci_ in 0..ncases {
        let case = gen(&mut rng);
        let nat = native::Native::new(&case.prog, case.l0);
        for si in 0..nstates {
            let mut st = State::random(&mut rng);
            (case.tweak)(&mut st, &mut rng);
            let (mut be, mut bn) = (Buf::new(), Buf::new());
            st.apply(&mut be, &case.ptr_regs);
            st.apply(&mut bn, &case.ptr_regs);
            let stop = run_emu(&case, &mut be);
            let code = nat.run(&mut bn);
            let ctx = || {
                format!(
                    "{name}: program {ci_} state {si}\nops: {:#?}\nstate: {st:#x?}",
                    case.prog.ops[NV + NX..case.prog.ops.len() - NX - NV - 1].to_vec()
                )
            };
            assert_eq!(stop, Stop::Ret(0), "emulator stop; {}", ctx());
            assert_eq!(code, 0, "native exit; {}", ctx());
            let (mut ne, mut nn) = (be.normalised(), bn.normalised());
            for &v in &case.skip_v {
                ne[VOUT + v] = 0;
                nn[VOUT + v] = 0;
            }
            if ne != nn {
                let i = ne.iter().zip(&nn).position(|(a, b)| a != b).unwrap();
                panic!(
                    "{name}: frame word {i} (V out {:?}, X out {:?}, mark {}, data {}) emu {:#018x} native {:#018x}\n{}",
                    i.checked_sub(VOUT).filter(|&k| k < NV),
                    i.checked_sub(XOUT).filter(|&k| k < NX),
                    i == MARK,
                    i >= DATA,
                    ne[i],
                    nn[i],
                    ctx()
                );
            }
            (case.model)(&st, &nn);
            total += 1;
        }
    }
    assert!(total >= 10_000, "{name}: only {total} states");
}

// ---- generators ----------------------------------------------------------------------------------------------

/// The signed/equality codes (valid for float compares too).
const FLOAT_CC: [Cc; 6] = [Cc::Eq, Cc::Ne, Cc::Lt, Cc::Le, Cc::Gt, Cc::Ge];
/// Every integer condition code, including the unsigned `Ult`/`Uge` (P0b).
const ALL_CC: [Cc; 8] = [
    Cc::Eq,
    Cc::Ne,
    Cc::Lt,
    Cc::Le,
    Cc::Gt,
    Cc::Ge,
    Cc::Ult,
    Cc::Uge,
];
const ALL_FOP: [FOp; 6] = [FOp::Add, FOp::Sub, FOp::Mul, FOp::Div, FOp::Min, FOp::Max];
const ALL_W: [W; 3] = [W::B1, W::B4, W::B8];

fn rv(rng: &mut Rng) -> V {
    rng.below(NV) as V
}
fn rx(rng: &mut Rng) -> X {
    rng.below(NX) as X
}
fn data_off(rng: &mut Rng, len: usize) -> usize {
    DATA * 8 + rng.below(DATA_BYTES - len - 256) + 128
}

/// A plain or register-based memory operand inside the data area of `len` bytes; registers used as bases are
/// pushed to `ptr`. Also returns the frame byte offset it designates.
fn mem_operand(rng: &mut Rng, len: usize, ptr: &mut Vec<V>) -> (M, usize) {
    let abs = data_off(rng, len);
    if rng.chance(2) {
        (fr(abs), abs)
    } else {
        let r = rv(rng);
        if !ptr.contains(&r) {
            ptr.push(r);
        }
        (
            M {
                base: Base::Reg(r),
                off: abs as i32 - PIVOT as i32,
            },
            abs,
        )
    }
}

/// A register that is not one of `ptr` (used as a stored value or index).
fn rv_not(rng: &mut Rng, ptr: &[V]) -> V {
    loop {
        let r = rv(rng);
        if !ptr.contains(&r) {
            return r;
        }
    }
}

fn width_bytes(w: W) -> usize {
    match w {
        W::B1 => 1,
        W::B4 => 4,
        W::B8 => 8,
    }
}

fn gen_ldf(rng: &mut Rng) -> Case {
    let mut ptr = Vec::new();
    let (m, _) = mem_operand(rng, 8, &mut ptr);
    let dst = rx(rng);
    let (prog, l0) = program(|_| vec![Op::LdF { dst, m }]);
    Case {
        ptr_regs: ptr,
        ..Case::new(prog, l0)
    }
}

fn gen_stf(rng: &mut Rng) -> Case {
    let mut ptr = Vec::new();
    let (m, _) = mem_operand(rng, 8, &mut ptr);
    let src = rx(rng);
    let (prog, l0) = program(|_| vec![Op::StF { m, src }]);
    Case {
        ptr_regs: ptr,
        ..Case::new(prog, l0)
    }
}

fn gen_movf(rng: &mut Rng) -> Case {
    let (d, s) = (rx(rng), rx(rng));
    let (prog, l0) = program(|_| vec![Op::MovF(d, s)]);
    Case::new(prog, l0)
}

/// What a correct machine must produce for `op` on non-NaN operands with a non-NaN result.
fn model_fop(op: FOp, a: f64, b: f64) -> f64 {
    match op {
        FOp::Add => a + b,
        FOp::Sub => a - b,
        FOp::Mul => a * b,
        FOp::Div => a / b,
        FOp::Min => {
            if a < b {
                a
            } else {
                b
            }
        }
        FOp::Max => {
            if a > b {
                a
            } else {
                b
            }
        }
    }
}

fn check_fbin(op: FOp, a: u64, b: u64, got: u64) {
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    // x86 NaN rules: first NaN operand (quieted), then the second, then the default NaN for invalid operations
    if matches!(op, FOp::Min | FOp::Max) {
        // minsd/maxsd: the second operand, bit for bit, unless the first is strictly smaller/larger
        let first = if op == FOp::Min { x < y } else { x > y };
        assert_eq!(got, if first { a } else { b }, "{op:?} {a:#x} {b:#x}");
    } else if x.is_nan() {
        assert_eq!(got, a | 0x0008_0000_0000_0000, "{op:?} nan first operand");
    } else if y.is_nan() {
        assert_eq!(got, b | 0x0008_0000_0000_0000, "{op:?} nan second operand");
    } else {
        let r = model_fop(op, x, y);
        if r.is_nan() {
            assert_eq!(
                got, 0xFFF8_0000_0000_0000,
                "{op:?} {a:#x} {b:#x} invalid operation"
            );
        } else {
            assert_eq!(got, r.to_bits(), "{op:?} {a:#x} {b:#x}");
        }
    }
}

fn gen_fbin(rng: &mut Rng) -> Case {
    let (op, dst, src) = (ALL_FOP[rng.below(6)], rx(rng), rx(rng));
    let (prog, l0) = program(|_| vec![Op::FBin { op, dst, src }]);
    Case {
        model: Box::new(move |st, out| {
            check_fbin(
                op,
                st.xin[dst as usize],
                st.xin[src as usize],
                out[XOUT + dst as usize],
            );
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_fbinm(rng: &mut Rng) -> Case {
    let (op, dst) = (ALL_FOP[rng.below(6)], rx(rng));
    let mut ptr = Vec::new();
    let (m, abs) = mem_operand(rng, 8, &mut ptr);
    let (prog, l0) = program(|_| vec![Op::FBinM { op, dst, m }]);
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| {
            if rng.chance(4) {
                // operand equal to the destination (or its negation): min/max/sub of equal values
                let mut b = st.xin[dst as usize];
                if rng.chance(2) {
                    b ^= 0x8000_0000_0000_0000;
                }
                st.patch(abs, &b.to_le_bytes());
            }
        }),
        model: Box::new(move |st, out| {
            let o = abs - DATA * 8;
            let b = u64::from_le_bytes(st.data[o..o + 8].try_into().unwrap());
            check_fbin(op, st.xin[dst as usize], b, out[XOUT + dst as usize]);
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_fsqrt(rng: &mut Rng) -> Case {
    let (d, s) = (rx(rng), rx(rng));
    let (prog, l0) = program(|_| vec![Op::FSqrt(d, s)]);
    Case {
        model: Box::new(move |st, out| {
            let a = st.xin[s as usize];
            let x = f64::from_bits(a);
            let got = out[XOUT + d as usize];
            if x.is_nan() {
                assert_eq!(got, a | 0x0008_0000_0000_0000);
            } else if x < 0.0 {
                assert_eq!(got, 0xFFF8_0000_0000_0000);
            } else {
                assert_eq!(got, x.sqrt().to_bits());
            }
        }),
        ..Case::new(prog, l0)
    }
}

/// F1: `roundsd` mode 9 (the emulator's `ffloor`).
fn gen_ffloor(rng: &mut Rng) -> Case {
    let (d, s) = (rx(rng), rx(rng));
    let (prog, l0) = program(|_| vec![Op::FFloor(d, s)]);
    Case {
        model: Box::new(move |st, out| {
            let a = st.xin[s as usize];
            let x = f64::from_bits(a);
            let got = out[XOUT + d as usize];
            if x.is_nan() {
                assert_eq!(got, a | 0x0008_0000_0000_0000);
            } else {
                assert_eq!(got, x.floor().to_bits());
            }
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_fneg(rng: &mut Rng) -> Case {
    let x = rx(rng);
    let (prog, l0) = program(|_| vec![Op::FNeg(x)]);
    Case::new(prog, l0)
}

fn gen_fabs(rng: &mut Rng) -> Case {
    let x = rx(rng);
    let (prog, l0) = program(|_| vec![Op::FAbs(x)]);
    Case::new(prog, l0)
}

fn gen_gtof(rng: &mut Rng) -> Case {
    let (x, v) = (rx(rng), rv(rng));
    let (prog, l0) = program(|_| vec![Op::GtoF(x, v)]);
    Case::new(prog, l0)
}

fn gen_ftog(rng: &mut Rng) -> Case {
    let (v, x) = (rv(rng), rx(rng));
    let (prog, l0) = program(|_| vec![Op::FtoG(v, x)]);
    Case::new(prog, l0)
}

fn gen_i2fx(rng: &mut Rng) -> Case {
    let (x, v) = (rx(rng), rv(rng));
    let (prog, l0) = program(|_| vec![Op::I2Fx(x, v)]);
    Case {
        model: Box::new(move |st, out| {
            assert_eq!(
                out[XOUT + x as usize],
                (st.vin[v as usize] as i64 as f64).to_bits()
            );
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_f2i(rng: &mut Rng) -> Case {
    let (dst, src) = (rv(rng), rx(rng));
    let (prog, l0) = program(|p| branchy(p, |to| Op::F2IExact { dst, src, to }));
    Case {
        tweak: Box::new(move |st, rng| {
            // many exactly integral values
            if rng.chance(2) {
                let v = match rng.below(3) {
                    0 => rng.below(2001) as i64 - 1000,
                    1 => rnd_i(rng) >> rng.below(12),
                    _ => (rng.next() as i64) >> rng.below(64),
                };
                st.xin[src as usize] = (v as f64).to_bits();
            }
        }),
        model: Box::new(move |st, out| {
            let x = f64::from_bits(st.xin[src as usize]);
            let exact = x.is_finite()
                && x.fract() == 0.0
                && (-9223372036854775808.0..9223372036854775808.0).contains(&x);
            // the marker: 1 = fell through (exact), 2 = jumped to the failure label
            assert_eq!(out[MARK], if exact { 1 } else { 2 }, "{x:?}");
            if exact {
                assert_eq!(out[VOUT + dst as usize], x as i64 as u64, "{x:?}");
            }
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_fbrx(rng: &mut Rng) -> Case {
    let (cc, a, b) = (FLOAT_CC[rng.below(6)], rx(rng), rx(rng));
    let (prog, l0) = program(|p| branchy(p, |to| Op::FBrX { cc, a, b, to }));
    Case {
        model: Box::new(move |st, out| {
            let (x, y) = (
                f64::from_bits(st.xin[a as usize]),
                f64::from_bits(st.xin[b as usize]),
            );
            let t = match cc {
                Cc::Eq => x == y,
                Cc::Ne => x != y,
                Cc::Lt => x < y,
                Cc::Le => x <= y,
                Cc::Gt => x > y,
                Cc::Ge => x >= y,
                Cc::Ult | Cc::Uge => unreachable!(),
            };
            assert_eq!(out[MARK], if t { 2 } else { 1 }, "{cc:?} {x:?} {y:?}");
        }),
        ..Case::new(prog, l0)
    }
}

fn brm_model(cc: Cc, w: W, mem: &[u8], imm: i32) -> bool {
    let (ua, ub) = match w {
        W::B1 => (u64::from(mem[0]), u64::from(imm as u8)),
        W::B4 => (
            u64::from(u32::from_le_bytes(mem[..4].try_into().unwrap())),
            u64::from(imm as u32),
        ),
        W::B8 => (
            u64::from_le_bytes(mem[..8].try_into().unwrap()),
            imm as i64 as u64,
        ),
    };
    let ord = match w {
        W::B1 => i64::from(mem[0] as i8).cmp(&i64::from(imm as u8 as i8)),
        W::B4 => i64::from(i32::from_le_bytes(mem[..4].try_into().unwrap())).cmp(&i64::from(imm)),
        W::B8 => i64::from_le_bytes(mem[..8].try_into().unwrap()).cmp(&i64::from(imm)),
    };
    use std::cmp::Ordering::*;
    match cc {
        Cc::Eq => ord == Equal,
        Cc::Ne => ord != Equal,
        Cc::Lt => ord == Less,
        Cc::Le => ord != Greater,
        Cc::Gt => ord == Greater,
        Cc::Ge => ord != Less,
        Cc::Ult => ua < ub,
        Cc::Uge => ua >= ub,
    }
}

fn rnd_imm(rng: &mut Rng) -> i32 {
    match rng.below(4) {
        0 => rng.below(300) as i32 - 150,
        1 => [
            0,
            1,
            -1,
            0x7F,
            0x80,
            0xFF,
            -128,
            0x7FFF_FFFF,
            i32::MIN,
            0x13,
            0x43,
        ][rng.below(11)],
        _ => rng.next() as i32,
    }
}

/// Make the compared memory equal to (or one off) the immediate half of the time.
fn match_imm(st: &mut State, rng: &mut Rng, abs: usize, w: W, imm: i32) {
    if rng.chance(2) {
        return;
    }
    let delta = [0i64, 0, 1, -1][rng.below(4)];
    let v = (i64::from(imm)).wrapping_add(delta);
    let bytes = v.to_le_bytes();
    st.patch(abs, &bytes[..width_bytes(w)]);
}

/// `Br` on two registers (or a register and an immediate) with edge values in both.
fn gen_br(rng: &mut Rng) -> Case {
    let cc = ALL_CC[rng.below(8)];
    let (a, bv) = (rv(rng), rv(rng));
    let imm = rnd_imm(rng);
    let use_imm = rng.chance(3);
    let b = if use_imm { Src::Imm(imm) } else { Src::V(bv) };
    let (prog, l0) = program(|p| branchy(p, |to| Op::Br { cc, a, b, to }));
    Case {
        tweak: Box::new(move |st, rng| {
            let edge = |rng: &mut Rng| match rng.below(3) {
                0 => SPECIAL_I[rng.below(SPECIAL_I.len())] as u64,
                1 => [0x8000_0000u64, 0x7FFF_FFFF, 0xFFFF_FFFF, 0x1_0000_0000][rng.below(4)],
                _ => rnd_i(rng) as u64,
            };
            st.vin[a as usize] = edge(rng);
            if rng.chance(3) {
                // equal or one off
                let d = [0u64, 1, u64::MAX][rng.below(3)];
                let other = match b {
                    Src::Imm(i) => i as i64 as u64,
                    Src::V(_) => edge(rng),
                };
                st.vin[a as usize] = other.wrapping_add(d);
            }
            if let Src::V(v) = b {
                if v != a {
                    st.vin[v as usize] = edge(rng);
                    if rng.chance(3) {
                        st.vin[v as usize] =
                            st.vin[a as usize].wrapping_add([0u64, 1, u64::MAX][rng.below(3)]);
                    }
                }
            }
        }),
        model: Box::new(move |st, out| {
            let x = st.vin[a as usize];
            let y = match b {
                Src::Imm(i) => i as i64 as u64,
                Src::V(v) => st.vin[v as usize],
            };
            let t = match cc {
                Cc::Eq => x == y,
                Cc::Ne => x != y,
                Cc::Lt => (x as i64) < (y as i64),
                Cc::Le => (x as i64) <= (y as i64),
                Cc::Gt => (x as i64) > (y as i64),
                Cc::Ge => (x as i64) >= (y as i64),
                Cc::Ult => x < y,
                Cc::Uge => x >= y,
            };
            assert_eq!(out[MARK], if t { 2 } else { 1 }, "{cc:?} {x:#x} {y:#x}");
        }),
        ..Case::new(prog, l0)
    }
}

/// `SetCi`/`SetK` to a pointer into the frame, then loads through `Base::Ci` / `Base::K` and a `BrM` on `Base::Ci`.
fn gen_setpin(rng: &mut Rng) -> Case {
    // (P2 bundle 1: `V10` is `r14`, the pinned constants register `SetK` writes and `Base::K` reads: not a case register here)
    let rv = |rng: &mut Rng| rng.below(NV - 1) as V;
    let (pc, pk) = (rv(rng), rv(rng));
    let (oc, ok) = (8 * rng.below(8) as i32, 8 * rng.below(8) as i32);
    let (dc, dk) = (rv(rng), rv(rng));
    let w = ALL_W[rng.below(3)];
    let off_c = DATA * 8 + oc as usize;
    let off_k = DATA * 8 + 8 * 16 + ok as usize;
    let rel = |o: usize| (o as i64 - PIVOT as i64) as i32;
    let (prog, l0) = program(|p| {
        let mut v = vec![
            // pointers: frame + PIVOT in pc / pk (the case registers are patched below)
            Op::SetCi(pc),
            Op::SetK(pk),
            Op::Ld {
                dst: dc,
                w: W::B8,
                m: M {
                    base: Base::Ci,
                    off: rel(off_c),
                },
            },
        ];
        if dk != dc {
            v.push(Op::Ld {
                dst: dk,
                w: W::B8,
                m: M {
                    base: Base::K,
                    off: rel(off_k),
                },
            });
        }
        v.extend(branchy(p, |to| Op::BrM {
            cc: Cc::Ult,
            w,
            m: M {
                base: Base::Ci,
                off: rel(off_c),
            },
            imm: 0x40,
            to,
        }));
        v
    });
    Case {
        ptr_regs: if pc == pk { vec![pc] } else { vec![pc, pk] },
        model: Box::new(move |st, out| {
            let ld = |o: usize| {
                u64::from_le_bytes(st.data[o - DATA * 8..o - DATA * 8 + 8].try_into().unwrap())
            };
            if dk != dc {
                assert_eq!(out[VOUT + dk as usize], ld(off_k));
                assert_eq!(out[VOUT + dc as usize], ld(off_c));
            }
        }),
        // (`SetK` leaves a pointer in `r14` = `V10` natively; the emulator keeps the pinned constants pointer elsewhere)
        skip_v: vec![10],
        ..Case::new(prog, l0)
    }
}

fn gen_brm(rng: &mut Rng) -> Case {
    let (cc, w, imm) = (ALL_CC[rng.below(8)], ALL_W[rng.below(3)], rnd_imm(rng));
    let mut ptr = Vec::new();
    let (m, abs) = mem_operand(rng, 8, &mut ptr);
    let (prog, l0) = program(|p| branchy(p, |to| Op::BrM { cc, w, m, imm, to }));
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| match_imm(st, rng, abs, w, imm)),
        model: Box::new(move |st, out| {
            let o = abs - DATA * 8;
            let t = brm_model(cc, w, &st.data[o..], imm);
            assert_eq!(
                out[MARK],
                if t { 2 } else { 1 },
                "{cc:?} {w:?} imm {imm:#x}"
            );
        }),
        ..Case::new(prog, l0)
    }
}

/// B4-3: `BrM16` (a 16-bit compare of memory, `Eq` / `Ne`).
fn gen_brm16(rng: &mut Rng) -> Case {
    let cc = [Cc::Eq, Cc::Ne][rng.below(2)];
    let imm = [0u16, 0x4403, 0x4504, 0xFFFF, 0x8000, 0x0100][rng.below(6)] ^ if rng.chance(2) { 0 } else { rng.next() as u16 };
    let mut ptr = Vec::new();
    let (m, abs) = mem_operand(rng, 8, &mut ptr);
    let (prog, l0) = program(|p| branchy(p, |to| Op::BrM16 { cc, m, imm, to }));
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| {
            if rng.chance(2) {
                let v = imm ^ [0u16, 0, 1, 0x100][rng.below(4)];
                st.patch(abs, &v.to_le_bytes());
            }
        }),
        model: Box::new(move |st, out| {
            let o = abs - DATA * 8;
            let v = u16::from_le_bytes([st.data[o], st.data[o + 1]]);
            let t = (v == imm) == (cc == Cc::Eq);
            assert_eq!(out[MARK], if t { 2 } else { 1 }, "{cc:?} imm {imm:#x}");
        }),
        ..Case::new(prog, l0)
    }
}

/// H3-F: `BrMR` (a 64-bit compare of memory with a register, `Eq` / `Ne`).
fn gen_brmr(rng: &mut Rng) -> Case {
    let cc = [Cc::Eq, Cc::Ne][rng.below(2)];
    let mut ptr = Vec::new();
    let (m, abs) = mem_operand(rng, 8, &mut ptr);
    let src = rv_not(rng, &ptr);
    let (prog, l0) = program(|p| branchy(p, |to| Op::BrMR { cc, m, src, to }));
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| {
            if rng.chance(2) {
                // equal, or different in one bit (high half included)
                let v = st.vin[src as usize] ^ if rng.chance(2) { 0 } else { 1u64 << rng.below(64) };
                st.patch(abs, &v.to_le_bytes());
            }
        }),
        model: Box::new(move |st, out| {
            let o = abs - DATA * 8;
            let v = u64::from_le_bytes(st.data[o..o + 8].try_into().unwrap());
            let t = (v == st.vin[src as usize]) == (cc == Cc::Eq);
            assert_eq!(out[MARK], if t { 2 } else { 1 }, "{cc:?}");
        }),
        ..Case::new(prog, l0)
    }
}

/// An indexed operand: base = a pointer register, index = another register set to a small signed value.
fn mi_operand(rng: &mut Rng, ptr: &mut Vec<V>) -> (Mi, V) {
    let base = rv(rng);
    ptr.push(base);
    let idx = loop {
        let r = rv(rng);
        if r != base {
            break r;
        }
    };
    let scale = [1u8, 2, 4, 8][rng.below(4)];
    let off = rng.below(129) as i32 - 64;
    (
        Mi {
            base,
            idx,
            scale,
            off,
        },
        idx,
    )
}

fn mi_abs(m: &Mi, idxval: i64) -> usize {
    (PIVOT as i64 + idxval * i64::from(m.scale) + i64::from(m.off)) as usize
}

fn small_idx(rng: &mut Rng) -> i64 {
    rng.below(33) as i64 - 16
}

fn gen_brmi(rng: &mut Rng) -> Case {
    let (cc, w, imm) = (ALL_CC[rng.below(8)], ALL_W[rng.below(3)], rnd_imm(rng));
    let mut ptr = Vec::new();
    let (m, idx) = mi_operand(rng, &mut ptr);
    let (prog, l0) = program(|p| branchy(p, |to| Op::BrMi { cc, w, m, imm, to }));
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| {
            let iv = small_idx(rng);
            st.vin[idx as usize] = iv as u64;
            match_imm(st, rng, mi_abs(&m, iv), w, imm);
        }),
        model: Box::new(move |st, out| {
            let o = mi_abs(&m, st.vin[idx as usize] as i64) - DATA * 8;
            let t = brm_model(cc, w, &st.data[o..], imm);
            assert_eq!(
                out[MARK],
                if t { 2 } else { 1 },
                "{cc:?} {w:?} imm {imm:#x}"
            );
        }),
        ..Case::new(prog, l0)
    }
}

fn gen_ldi(rng: &mut Rng) -> Case {
    let (w, dst) = (ALL_W[rng.below(3)], rv(rng));
    let mut ptr = Vec::new();
    let (m, idx) = mi_operand(rng, &mut ptr);
    let (prog, l0) = program(|_| vec![Op::LdI { dst, w, m }]);
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| st.vin[idx as usize] = small_idx(rng) as u64),
        ..Case::new(prog, l0)
    }
}

fn gen_sti(rng: &mut Rng) -> Case {
    let w = ALL_W[rng.below(3)];
    let mut ptr = Vec::new();
    let (m, idx) = mi_operand(rng, &mut ptr);
    let src = if rng.chance(3) {
        Src::Imm(rnd_imm(rng))
    } else {
        // a register that is neither the pointer nor the index (a pointer would differ between the buffers)
        let mut r = rv_not(rng, &ptr);
        while r == idx {
            r = rv_not(rng, &ptr);
        }
        Src::V(r)
    };
    let (prog, l0) = program(|_| vec![Op::StI { w, m, src }]);
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| st.vin[idx as usize] = small_idx(rng) as u64),
        ..Case::new(prog, l0)
    }
}

fn gen_ldfi(rng: &mut Rng) -> Case {
    let dst = rx(rng);
    let mut ptr = Vec::new();
    let (m, idx) = mi_operand(rng, &mut ptr);
    let (prog, l0) = program(|_| vec![Op::LdFi { dst, m }]);
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| st.vin[idx as usize] = small_idx(rng) as u64),
        ..Case::new(prog, l0)
    }
}

fn gen_stfi(rng: &mut Rng) -> Case {
    let src = rx(rng);
    let mut ptr = Vec::new();
    let (m, idx) = mi_operand(rng, &mut ptr);
    let (prog, l0) = program(|_| vec![Op::StFi { m, src }]);
    Case {
        ptr_regs: ptr,
        tweak: Box::new(move |st, rng| st.vin[idx as usize] = small_idx(rng) as u64),
        ..Case::new(prog, l0)
    }
}

/// Store ops with plain operands (immediate and register sources, all widths) next to `St`.
fn gen_st_plain(rng: &mut Rng) -> Case {
    let w = ALL_W[rng.below(3)];
    let mut ptr = Vec::new();
    let (m, _) = mem_operand(rng, 8, &mut ptr);
    let src = if rng.chance(3) {
        Src::Imm(rnd_imm(rng))
    } else {
        Src::V(rv_not(rng, &ptr))
    };
    let (prog, l0) = program(|_| vec![Op::St { w, m, src }]);
    Case {
        ptr_regs: ptr,
        ..Case::new(prog, l0)
    }
}

/// The old division ops with V7/V8 live: they clobber `rax`/`rdx`, and the emulator models exactly that.
fn gen_divrem(rng: &mut Rng) -> Case {
    let regs: Vec<V> = {
        let mut v: Vec<V> = (0..7).collect();
        for i in (1..7).rev() {
            v.swap(i, rng.below(i + 1));
        }
        v
    };
    let (q, r, a, b) = (regs[0], regs[1], regs[2], regs[3]);
    let unsigned = rng.chance(2);
    let (prog, l0) = program(|_| {
        if unsigned {
            vec![Op::UDiv { dst: q, a, b }]
        } else {
            vec![Op::SDivRem { q, r, a, b }]
        }
    });
    Case {
        skip_v: vec![7, 8],
        tweak: Box::new(move |st, rng| {
            // the template guarantees b != 0 and b != -1
            let mut d = st.vin[b as usize];
            if d == 0 || (!unsigned && d == u64::MAX) {
                d = 3 + rng.below(100) as u64;
            }
            st.vin[b as usize] = d;
            if a == b {
                st.vin[a as usize] = d;
            }
        }),
        ..Case::new(prog, l0)
    }
}

/// B4-4: the high half of a 64 x 64 multiplication (`MulHiU`): operands in `V0..V6`, results too; V7/V8 are clobbered.
fn gen_mulhi(rng: &mut Rng) -> Case {
    let a = rng.below(7) as V;
    let dst = rng.below(7) as V;
    let k = match rng.below(4) {
        0 => u64::MAX,
        1 => 0xAAAA_AAAA_AAAA_AAAB,
        _ => rng.next(),
    };
    let (prog, l0) = program(|_| vec![Op::MulHiU { dst, a, k }]);
    Case {
        skip_v: vec![7, 8],
        ..Case::new(prog, l0)
    }
}

/// A sequence of several new ops (register allocation interplay: V7/V8 and all of X as ordinary registers).
fn gen_mix(rng: &mut Rng) -> Case {
    let mut ptr = Vec::new();
    let (mld, _) = mem_operand(rng, 8, &mut ptr);
    let (mst, _) = mem_operand(rng, 8, &mut ptr);
    let (a, b, c) = (rx(rng), rx(rng), rx(rng));
    let v1 = rv_not(rng, &ptr);
    let (prog, l0) = program(|_| {
        vec![
            Op::LdF { dst: a, m: mld },
            Op::FBin {
                op: ALL_FOP[rng.below(6)],
                dst: a,
                src: b,
            },
            Op::MovF(c, a),
            Op::FSqrt(b, c),
            Op::FtoG(v1, b),
            Op::I2Fx(a, v1),
            Op::FBinM {
                op: FOp::Mul,
                dst: a,
                m: mld,
            },
            Op::StF { m: mst, src: a },
        ]
    });
    Case {
        ptr_regs: ptr,
        ..Case::new(prog, l0)
    }
}

macro_rules! diff_tests {
    ($($name:ident: $gen:ident, $ncases:expr, $nstates:expr;)*) => {
        $(
            #[cfg(not(miri))]
            #[test]
            fn $name() {
                drive(stringify!($name), $ncases, $nstates, $gen);
            }
        )*
    };
}

diff_tests! {
    diff_ldf: gen_ldf, 40, 300;
    diff_stf: gen_stf, 40, 300;
    diff_movf: gen_movf, 40, 300;
    diff_fbin: gen_fbin, 120, 300;
    diff_fbinm: gen_fbinm, 120, 300;
    diff_fsqrt: gen_fsqrt, 40, 300;
    diff_ffloor: gen_ffloor, 40, 300;
    diff_fneg: gen_fneg, 40, 300;
    diff_fabs: gen_fabs, 40, 300;
    diff_gtof: gen_gtof, 40, 300;
    diff_ftog: gen_ftog, 40, 300;
    diff_i2fx: gen_i2fx, 40, 300;
    diff_f2i_exact: gen_f2i, 60, 300;
    diff_fbrx: gen_fbrx, 120, 300;
    diff_br: gen_br, 120, 300;
    diff_setci_setk: gen_setpin, 60, 300;
    diff_brm: gen_brm, 120, 300;
    diff_brm16: gen_brm16, 120, 300;
    diff_brmr: gen_brmr, 120, 300;
    diff_brmi: gen_brmi, 120, 300;
    diff_ldi: gen_ldi, 60, 300;
    diff_sti: gen_sti, 60, 300;
    diff_ldfi: gen_ldfi, 40, 300;
    diff_stfi: gen_stfi, 40, 300;
    diff_st_plain: gen_st_plain, 60, 300;
    diff_divrem_keeps_v7_v8_clobber_model: gen_divrem, 80, 200;
    diff_mulhi: gen_mulhi, 80, 300;
    diff_mix: gen_mix, 60, 300;
}

// ---- known answers (emulator only; these also run under Miri) ----------------------------------------------

/// Run `body` between the standard prologue/epilogue on a state with the given X/V inputs; returns the buffer.
fn run_known(
    vin: &[(usize, u64)],
    xin: &[(usize, u64)],
    data: &[u8],
    body: impl FnOnce(&mut Program) -> Vec<Op>,
) -> Buf {
    let (prog, l0) = program(body);
    let mut b = Buf::new();
    for &(i, v) in vin {
        b.frame[VIN + i] = v;
    }
    for &(i, v) in xin {
        b.frame[XIN + i] = v;
    }
    // SAFETY: DATA*8 + data.len() stays inside the frame buffer (callers pass short slices).
    unsafe {
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            b.frame.as_mut_ptr().cast::<u8>().add(DATA * 8),
            data.len(),
        );
    }
    let env = b.env();
    // SAFETY: as in `run_emu`.
    let stop = unsafe { emu::run(&prog, l0, &env, 100_000) };
    assert_eq!(stop, Stop::Ret(0));
    b
}

fn xo(b: &Buf, i: usize) -> u64 {
    b.frame[XOUT + i]
}
fn vo(b: &Buf, i: usize) -> u64 {
    b.frame[VOUT + i]
}

const NAN_Q: u64 = 0x7FF8_0000_0000_0000;
const NAN_S: u64 = 0x7FF0_0000_0000_0001;
const NAN_D: u64 = 0xFFF8_0000_0000_0000;
const INF: u64 = 0x7FF0_0000_0000_0000;
const NINF: u64 = 0xFFF0_0000_0000_0000;
const PZ: u64 = 0;
const NZ: u64 = 0x8000_0000_0000_0000;

fn f(x: f64) -> u64 {
    x.to_bits()
}

fn bin(op: FOp, a: u64, b: u64) -> u64 {
    let r = run_known(&[], &[(2, a), (3, b)], &[], |_| {
        vec![Op::FBin { op, dst: 2, src: 3 }]
    });
    assert_eq!(xo(&r, 3), b, "source unchanged");
    xo(&r, 2)
}

#[test]
fn fbin_ieee_semantics() {
    assert_eq!(bin(FOp::Add, f(1.5), f(2.25)), f(3.75));
    assert_eq!(bin(FOp::Sub, f(1.5), f(2.25)), f(-0.75));
    assert_eq!(bin(FOp::Mul, f(-3.0), f(0.5)), f(-1.5));
    assert_eq!(bin(FOp::Div, f(1.0), f(4.0)), f(0.25));
    assert_eq!(bin(FOp::Div, f(1.0), PZ), INF);
    assert_eq!(bin(FOp::Div, f(-1.0), PZ), NINF);
    assert_eq!(bin(FOp::Add, PZ, NZ), PZ);
    assert_eq!(bin(FOp::Add, NZ, NZ), NZ);
    assert_eq!(bin(FOp::Sub, NZ, PZ), NZ);
    // invalid operations give the default NaN (sign bit set)
    assert_eq!(bin(FOp::Sub, INF, INF), NAN_D);
    assert_eq!(bin(FOp::Div, PZ, PZ), NAN_D);
    assert_eq!(bin(FOp::Mul, INF, PZ), NAN_D);
    // NaN operands: the first wins, a signalling NaN is quieted, payloads survive
    assert_eq!(bin(FOp::Add, NAN_Q | 5, f(1.0)), NAN_Q | 5);
    assert_eq!(bin(FOp::Add, f(1.0), NAN_Q | 7), NAN_Q | 7);
    assert_eq!(bin(FOp::Add, NAN_Q | 5, NAN_Q | 7), NAN_Q | 5);
    assert_eq!(bin(FOp::Mul, NAN_S, f(1.0)), NAN_S | 0x0008_0000_0000_0000);
    assert_eq!(bin(FOp::Div, f(2.0), NAN_S), NAN_S | 0x0008_0000_0000_0000);
    assert_eq!(bin(FOp::Mul, NAN_Q | NZ, f(1.0)), NAN_Q | NZ);
    // denormals are not flushed
    assert_eq!(bin(FOp::Mul, 1, f(1.0)), 1);
    assert_eq!(bin(FOp::Sub, 2, 1), 1);
    assert_eq!(bin(FOp::Div, 2, f(2.0)), 1);
}

#[test]
fn min_max_are_minsd_maxsd() {
    // the second operand wins on NaN and on equality (so signed zeros pick the second)
    assert_eq!(bin(FOp::Min, f(1.0), f(2.0)), f(1.0));
    assert_eq!(bin(FOp::Min, f(2.0), f(1.0)), f(1.0));
    assert_eq!(bin(FOp::Max, f(1.0), f(2.0)), f(2.0));
    assert_eq!(bin(FOp::Max, f(2.0), f(1.0)), f(2.0));
    assert_eq!(bin(FOp::Min, PZ, NZ), NZ);
    assert_eq!(bin(FOp::Min, NZ, PZ), PZ);
    assert_eq!(bin(FOp::Max, PZ, NZ), NZ);
    assert_eq!(bin(FOp::Max, NZ, PZ), PZ);
    assert_eq!(bin(FOp::Min, NAN_Q, f(1.0)), f(1.0)); // NaN first operand: second wins
    assert_eq!(bin(FOp::Max, f(1.0), NAN_S), NAN_S); // returned unquieted
    assert_eq!(bin(FOp::Min, NAN_S, NAN_Q | 3), NAN_Q | 3);
    assert_eq!(bin(FOp::Max, f(1.0), NAN_Q | 3), NAN_Q | 3);
    assert_eq!(bin(FOp::Min, NINF, INF), NINF);
    assert_eq!(bin(FOp::Max, NINF, INF), INF);
}

#[test]
fn fbin_memory_operand_and_loads_and_stores() {
    let data = f(10.0).to_le_bytes();
    let r = run_known(&[], &[(4, f(2.5))], &data, |_| {
        vec![
            Op::FBinM {
                op: FOp::Mul,
                dst: 4,
                m: fr(DATA * 8),
            },
            Op::LdF {
                dst: 5,
                m: fr(DATA * 8),
            },
            Op::StF {
                m: fr(DATA * 8 + 8),
                src: 4,
            },
            Op::MovF(6, 4),
        ]
    });
    assert_eq!(xo(&r, 4), f(25.0));
    assert_eq!(xo(&r, 5), f(10.0));
    assert_eq!(xo(&r, 6), f(25.0));
    assert_eq!(r.frame[DATA + 1], f(25.0));
    // loads and stores copy NaN payloads bit for bit
    let r = run_known(&[], &[(15, NAN_S | NZ | 0xABC)], &[], |_| {
        vec![
            Op::StF {
                m: fr(DATA * 8),
                src: 15,
            },
            Op::LdF {
                dst: 9,
                m: fr(DATA * 8),
            },
        ]
    });
    assert_eq!(xo(&r, 9), NAN_S | NZ | 0xABC);
}

#[test]
fn sqrt_neg_abs_and_moves() {
    let sq = |a| {
        xo(
            &run_known(&[], &[(3, a)], &[], |_| vec![Op::FSqrt(7, 3)]),
            7,
        )
    };
    assert_eq!(sq(f(9.0)), f(3.0));
    assert_eq!(sq(NZ), NZ);
    assert_eq!(sq(PZ), PZ);
    assert_eq!(sq(f(-1.0)), NAN_D);
    assert_eq!(sq(NINF), NAN_D);
    assert_eq!(sq(INF), INF);
    assert_eq!(sq(NAN_S), NAN_S | 0x0008_0000_0000_0000);
    assert_eq!(sq(1), f(f64::from_bits(1).sqrt()));
    let neg = |a| xo(&run_known(&[], &[(8, a)], &[], |_| vec![Op::FNeg(8)]), 8);
    assert_eq!(neg(PZ), NZ);
    assert_eq!(neg(NZ), PZ);
    assert_eq!(neg(f(1.5)), f(-1.5));
    assert_eq!(neg(NAN_Q | 9), NAN_Q | 9 | NZ);
    let abs = |a| xo(&run_known(&[], &[(8, a)], &[], |_| vec![Op::FAbs(8)]), 8);
    assert_eq!(abs(NZ), PZ);
    assert_eq!(abs(f(-1.5)), f(1.5));
    assert_eq!(abs(NAN_Q | NZ | 9), NAN_Q | 9);
    // FNeg/FAbs clobber V7 and X0 (documented), and nothing else
    let r = run_known(&[(7, 55), (1, 66)], &[(0, 77)], &[], |_| vec![Op::FNeg(8)]);
    assert_eq!(vo(&r, 1), 66);
    assert_ne!(vo(&r, 7), 55);
    // G <-> F moves and integer conversion
    let r = run_known(
        &[(8, 0x1122_3344_5566_7788), (2, (-5i64) as u64)],
        &[(3, NAN_S | 0x77)],
        &[],
        |_| vec![Op::GtoF(1, 8), Op::FtoG(4, 3), Op::I2Fx(5, 2)],
    );
    assert_eq!(xo(&r, 1), 0x1122_3344_5566_7788);
    assert_eq!(vo(&r, 4), NAN_S | 0x77);
    assert_eq!(xo(&r, 5), f(-5.0));
    let r = run_known(&[(1, i64::MAX as u64)], &[], &[], |_| vec![Op::I2Fx(9, 1)]);
    assert_eq!(xo(&r, 9), f(9223372036854775808.0));
}

#[test]
fn f2i_exact_and_fbrx() {
    let cv = |a: u64| {
        let r = run_known(&[], &[(2, a)], &[], |p| {
            let (t, e) = (p.label(), p.label());
            let mark = |v| Op::St {
                w: W::B8,
                m: fr(8 * MARK),
                src: Src::Imm(v),
            };
            vec![
                Op::F2IExact {
                    dst: 5,
                    src: 2,
                    to: t,
                },
                mark(1),
                Op::Jmp(e),
                Op::Bind(t),
                mark(2),
                Op::Bind(e),
            ]
        });
        (r.frame[MARK] == 1, vo(&r, 5))
    };
    assert_eq!(cv(f(3.0)), (true, 3));
    assert_eq!(cv(f(-3.0)), (true, (-3i64) as u64));
    assert_eq!(cv(NZ), (true, 0));
    assert_eq!(cv(f(3.5)), (false, 3));
    assert_eq!(cv(f(-3.5)), (false, (-3i64) as u64));
    assert_eq!(cv(f(-9223372036854775808.0)), (true, i64::MIN as u64));
    assert_eq!(cv(f(9223372036854775808.0)), (false, i64::MIN as u64));
    assert_eq!(cv(f(9223372036854774784.0)), (true, 9223372036854774784));
    assert_eq!(cv(NAN_Q), (false, i64::MIN as u64));
    assert_eq!(cv(INF), (false, i64::MIN as u64));
    assert_eq!(cv(NINF), (false, i64::MIN as u64));
    assert_eq!(cv(f(1e300)), (false, i64::MIN as u64));
    // FBrX: NaN makes everything false except `Ne`
    let br = |cc, a, b| {
        let r = run_known(&[], &[(2, a), (14, b)], &[], |p| {
            branchy(p, |to| Op::FBrX {
                cc,
                a: 2,
                b: 14,
                to,
            })
        });
        r.frame[MARK] == 2
    };
    for cc in FLOAT_CC {
        assert_eq!(br(cc, NAN_Q, f(1.0)), cc == Cc::Ne, "{cc:?}");
        assert_eq!(br(cc, f(1.0), NAN_Q), cc == Cc::Ne, "{cc:?}");
        assert_eq!(br(cc, NAN_Q, NAN_Q), cc == Cc::Ne, "{cc:?}");
    }
    assert!(br(Cc::Eq, PZ, NZ));
    assert!(!br(Cc::Lt, PZ, NZ));
    assert!(br(Cc::Le, NZ, PZ));
    assert!(br(Cc::Lt, NINF, f(-1e308)));
    assert!(br(Cc::Gt, INF, f(1e308)));
    assert!(br(Cc::Ge, f(2.0), f(2.0)));
    assert!(!br(Cc::Gt, f(2.0), f(2.0)));
}

#[test]
fn brm_widths_are_signed_at_their_width() {
    let br = |cc, w, mem: u64, imm: i32| {
        let r = run_known(&[], &[], &mem.to_le_bytes(), |p| {
            branchy(p, |to| Op::BrM {
                cc,
                w,
                m: fr(DATA * 8),
                imm,
                to,
            })
        });
        r.frame[MARK] == 2
    };
    // bytes: 0x80 is -128 (below 0), 0x13 > 0x12
    assert!(br(Cc::Lt, W::B1, 0x80, 0));
    assert!(br(Cc::Eq, W::B1, 0x80, -128));
    assert!(br(Cc::Eq, W::B1, 0x80, 0x80));
    assert!(br(Cc::Gt, W::B1, 0x13, 0x12));
    assert!(br(Cc::Eq, W::B1, 0xFFFF_FF13, 0x13)); // only the byte is compared
    assert!(br(Cc::Ne, W::B1, 0x0113, 0x14));
    // dwords: 0x8000_0000 is i32::MIN
    assert!(br(Cc::Lt, W::B4, 0x8000_0000, 0));
    assert!(br(Cc::Eq, W::B4, 0xFFFF_FFFF, -1));
    assert!(br(Cc::Eq, W::B4, 0x1_0000_0005, 5)); // upper dword ignored
                                                  // qwords: the immediate is sign-extended
    assert!(br(Cc::Eq, W::B8, u64::MAX, -1));
    assert!(!br(Cc::Eq, W::B8, 0xFFFF_FFFF, -1));
    assert!(br(Cc::Ge, W::B8, 0, 0));
    assert!(br(Cc::Gt, W::B8, 0x1_0000_0000, i32::MAX));
}

#[test]
fn indexed_operands() {
    let mut data = Vec::new();
    for i in 0..16u64 {
        data.extend_from_slice(&(0x100 + i).to_le_bytes());
    }
    let base = |b: &Buf| b.base() + (DATA * 8) as u64;
    // we need the buffer's address inside the program input: run in two steps (address is known after `Buf::new`)
    let (prog, l0) = program(|p| {
        let m = Mi {
            base: 1,
            idx: 2,
            scale: 8,
            off: 8,
        };
        let mut v = vec![
            Op::LdI {
                dst: 3,
                w: W::B8,
                m,
            },
            Op::LdI {
                dst: 4,
                w: W::B1,
                m: Mi {
                    scale: 1,
                    off: 14,
                    ..m
                },
            },
            Op::LdFi {
                dst: 5,
                m: Mi {
                    scale: 8,
                    off: 0,
                    ..m
                },
            },
            Op::StI {
                w: W::B4,
                m: Mi {
                    scale: 4,
                    off: 64,
                    ..m
                },
                src: Src::Imm(-1),
            },
            Op::StFi {
                m: Mi {
                    scale: 8,
                    off: 80,
                    ..m
                },
                src: 5,
            },
        ];
        v.extend(branchy(p, |to| Op::BrMi {
            cc: Cc::Eq,
            w: W::B8,
            m,
            imm: 0x103,
            to,
        }));
        v
    });
    let mut b = Buf::new();
    b.frame[DATA..DATA + 16].copy_from_slice(&(0..16).map(|i| 0x100 + i).collect::<Vec<u64>>());
    b.frame[VIN + 1] = base(&b); // base register
    b.frame[VIN + 2] = 2; // index
    let env = b.env();
    // SAFETY: all addresses lie inside the data area of the frame buffer.
    assert_eq!(unsafe { emu::run(&prog, l0, &env, 10_000) }, Stop::Ret(0));
    assert_eq!(vo(&b, 3), 0x103, "[base + 2*8 + 8]");
    assert_eq!(
        vo(&b, 4),
        0x02,
        "byte [base + 2*1 + 14] = low byte of word 2"
    );
    assert_eq!(xo(&b, 5), 0x102);
    assert_eq!(
        b.frame[DATA + 9] as u32,
        u32::MAX,
        "dword store at base + 2*4 + 64 = word 9"
    );
    assert_eq!(
        b.frame[DATA + 10 + 2],
        0x102,
        "StFi at base + 16 + 80 = word 12"
    );
    assert_eq!(b.frame[MARK], 2, "BrMi taken (0x103 == 0x103)");
}

#[test]
fn run_until_stops_at_the_first_stop_label() {
    let mut p = Program::default();
    let (l0, l1, l2, l3) = (p.label(), p.label(), p.label(), p.label());
    for op in [
        Op::Bind(l0),
        Op::Mov(0, 1),
        Op::Bind(l1), // a block head
        Op::Add(0, Src::Imm(10)),
        Op::Br {
            cc: Cc::Lt,
            a: 0,
            b: Src::Imm(100),
            to: l1,
        }, // loops back through l1
        Op::Bind(l2),
        Op::Mov(1, 7),
        Op::Bind(l3),
        Op::Ret(3),
    ] {
        p.push(op);
    }
    let mut b = Buf::new();
    let env = b.env();
    // SAFETY: the program touches no memory.
    let run = |stop: &[L], start: L| unsafe { emu::run_until(&p, start, stop, &env, 10_000) };
    // empty list: behaves like `run`
    assert_eq!(run(&[], l0), Until::Stop(Stop::Ret(3)));
    // the start label itself never stops, but a back edge to it does
    assert_eq!(run(&[l0, l1], l0), Until::Label(l1));
    assert_eq!(run(&[l1], l1), Until::Label(l1)); // first Bind skipped, the loop comes back
    assert_eq!(run(&[l2], l0), Until::Label(l2));
    assert_eq!(run(&[l3, l2], l0), Until::Label(l2));
    assert_eq!(run(&[l3], l0), Until::Label(l3));
    assert_eq!(run(&[l0], l1), Until::Stop(Stop::Ret(3)));
    // out of steps is reported as before
    // SAFETY: the program touches no memory.
    let few = unsafe { emu::run_until(&p, l0, &[], &env, 3) };
    assert_eq!(few, Until::Stop(Stop::OutOfSteps));
}

#[test]
fn division_poisons_v7_v8() {
    // the emulator poisons V7/V8 (natively rax/rdx hold the quotient/remainder)
    let r = run_known(
        &[(0, (-7i64) as u64), (1, 2), (7, 111), (8, 222)],
        &[],
        &[],
        |_| {
            vec![Op::SDivRem {
                q: 2,
                r: 3,
                a: 0,
                b: 1,
            }]
        },
    );
    assert_eq!((vo(&r, 2) as i64, vo(&r, 3) as i64), (-3, -1));
    assert_eq!((vo(&r, 7), vo(&r, 8)), (emu::POISON, emu::POISON));
    let r = run_known(&[(0, 100), (1, 7), (7, 111), (8, 222)], &[], &[], |_| {
        vec![Op::UDiv { dst: 2, a: 0, b: 1 }]
    });
    assert_eq!(
        (vo(&r, 2), vo(&r, 7), vo(&r, 8)),
        (14, emu::POISON, emu::POISON)
    );
}

#[test]
fn random_emulator_against_models() {
    // Miri-sized version of the differential tests: the emulator against plain Rust models
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..40 {
        let case = gen_fbin(&mut rng);
        for _ in 0..8 {
            let st = State::random(&mut rng);
            let mut b = Buf::new();
            st.apply(&mut b, &case.ptr_regs);
            assert_eq!(run_emu(&case, &mut b), Stop::Ret(0));
            (case.model)(&st, &b.frame);
        }
        for gen in [
            gen_fbinm, gen_fsqrt, gen_ffloor, gen_f2i, gen_fbrx, gen_br, gen_setpin, gen_brm, gen_brmi,
            gen_i2fx,
        ] {
            let case = gen(&mut rng);
            for _ in 0..6 {
                let mut st = State::random(&mut rng);
                (case.tweak)(&mut st, &mut rng);
                let mut b = Buf::new();
                st.apply(&mut b, &case.ptr_regs);
                assert_eq!(run_emu(&case, &mut b), Stop::Ret(0));
                (case.model)(&st, &b.frame);
            }
        }
    }
}

// ---- P0b: unsigned condition codes, SetCi/SetK/JmpR -----------------------------------------------------------

/// A helper model for programs that call none.
struct NoH;
impl emu::Helpers for NoH {
    fn supported(&self) -> bool {
        false
    }
    unsafe fn call(
        &mut self,
        _f: u64,
        _a: u32,
        _b: u32,
        _env: &Env,
        _frame: *mut u8,
    ) -> emu::HelperOut {
        emu::HelperOut {
            result: 0,
            new_frame: None,
        }
    }
}

fn mk(base: Base, off: i32) -> M {
    M { base, off }
}

#[cfg(not(miri))]
const EDGE_U: [u64; 10] = [
    0,
    1,
    2,
    u64::MAX,
    u64::MAX - 1,
    i64::MAX as u64,
    i64::MIN as u64,
    (i64::MIN as u64) + 1,
    0xFFFF_FFFF,
    0x1_0000_0000,
];

#[test]
fn unsigned_cc_known_answers() {
    let br = |cc, a: u64, b: Src| {
        let bv = if let Src::Imm(i) = b {
            i as i64 as u64
        } else {
            0
        };
        let r = run_known(&[(0, a), (1, bv)], &[], &[], |p| {
            branchy(p, |to| Op::Br { cc, a: 0, b, to })
        });
        r.frame[MARK] == 2
    };
    // -1 is the largest unsigned value; the immediate sign-extends
    assert!(!br(Cc::Ult, u64::MAX, Src::Imm(0)));
    assert!(br(Cc::Uge, u64::MAX, Src::Imm(0)));
    assert!(br(Cc::Ult, 0, Src::Imm(-1)));
    assert!(br(Cc::Ult, i64::MAX as u64, Src::Imm(-1)));
    assert!(br(Cc::Lt, u64::MAX, Src::Imm(0)) && !br(Cc::Ult, u64::MAX, Src::Imm(0)));
    // the unsigned bounds check: idx <u n
    for (idx, n, inb) in [
        (0u64, 5u64, true),
        (4, 5, true),
        (5, 5, false),
        (u64::MAX, 5, false),
        (i64::MIN as u64, 5, false),
    ] {
        let t = |cc| {
            let r = run_known(&[(0, idx), (1, n)], &[], &[], |p| {
                branchy(p, |to| Op::Br {
                    cc,
                    a: 0,
                    b: Src::V(1),
                    to,
                })
            });
            r.frame[MARK] == 2
        };
        assert_eq!(t(Cc::Ult), inb, "{idx:#x} <u {n}");
        assert_eq!(t(Cc::Uge), !inb, "{idx:#x} >=u {n}");
    }
    // BrM at each width, unsigned at the width of the access
    let brm = |cc, w, mem: u64, imm: i32| {
        let r = run_known(&[], &[], &mem.to_le_bytes(), |p| {
            branchy(p, |to| Op::BrM {
                cc,
                w,
                m: fr(DATA * 8),
                imm,
                to,
            })
        });
        r.frame[MARK] == 2
    };
    assert!(brm(Cc::Uge, W::B1, 0x80, 0x7F));
    assert!(brm(Cc::Lt, W::B1, 0x80, 0x7F));
    assert!(brm(Cc::Ult, W::B1, 0x7F, -128)); // imm as u8 = 0x80
    assert!(brm(Cc::Uge, W::B4, 0x8000_0000, 1));
    assert!(brm(Cc::Ult, W::B4, 0x1_0000_0001, 2)); // upper dword ignored
    assert!(brm(Cc::Ult, W::B8, 5, -1));
    assert!(brm(Cc::Uge, W::B8, u64::MAX, i32::MAX));
}

/// Native against emulator for `Br` over all pairs of edge values and every condition code.
#[cfg(not(miri))]
#[test]
fn diff_br_edge_pairs() {
    for cc in ALL_CC {
        for use_imm in [false, true] {
            let (prog, l0) = program(|p| {
                branchy(p, |to| Op::Br {
                    cc,
                    a: 0,
                    b: if use_imm { Src::Imm(-1) } else { Src::V(1) },
                    to,
                })
            });
            let case = Case::new(prog, l0);
            let nat = native::Native::new(&case.prog, case.l0);
            for &x in &EDGE_U {
                for &y in &EDGE_U {
                    let (mut be, mut bn) = (Buf::new(), Buf::new());
                    for b in [&mut be, &mut bn] {
                        b.frame[VIN] = x;
                        b.frame[VIN + 1] = y;
                    }
                    assert_eq!(run_emu(&case, &mut be), Stop::Ret(0));
                    assert_eq!(nat.run(&mut bn), 0);
                    assert_eq!(
                        be.frame[MARK], bn.frame[MARK],
                        "{cc:?} {x:#x} {y:#x} imm {use_imm}"
                    );
                    let yy = if use_imm { u64::MAX } else { y };
                    let want = match cc {
                        Cc::Eq => x == yy,
                        Cc::Ne => x != yy,
                        Cc::Lt => (x as i64) < (yy as i64),
                        Cc::Le => (x as i64) <= (yy as i64),
                        Cc::Gt => (x as i64) > (yy as i64),
                        Cc::Ge => (x as i64) >= (yy as i64),
                        Cc::Ult => x < yy,
                        Cc::Uge => x >= yy,
                    };
                    assert_eq!(
                        bn.frame[MARK],
                        if want { 2 } else { 1 },
                        "{cc:?} {x:#x} {yy:#x}"
                    );
                }
            }
        }
    }
}

#[test]
fn setci_setk_redirect_base_ci_and_base_k() {
    let mut data = [0u8; 32];
    data[..8].copy_from_slice(&0x1111_2222_3333_4444u64.to_le_bytes());
    data[8..16].copy_from_slice(&0x5555_6666_7777_8888u64.to_le_bytes());
    let r = run_known(&[], &[], &data, |_| {
        vec![
            Op::Lea(1, fr(DATA * 8)),
            Op::SetCi(1),
            Op::Lea(2, fr(DATA * 8 + 8)),
            Op::SetK(2),
            Op::Ld {
                dst: 3,
                w: W::B8,
                m: mk(Base::Ci, 0),
            },
            Op::Ld {
                dst: 4,
                w: W::B8,
                m: mk(Base::K, 0),
            },
            Op::Ld {
                dst: 5,
                w: W::B4,
                m: mk(Base::Ci, 12),
            },
        ]
    });
    assert_eq!(vo(&r, 3), 0x1111_2222_3333_4444);
    assert_eq!(vo(&r, 4), 0x5555_6666_7777_8888);
    assert_eq!(vo(&r, 5), 0x5555_6666);
}

#[test]
fn setci_setk_state_is_visible_across_run_until() {
    let mut b = Buf::new();
    let env = b.env();
    let mut p = Program::default();
    let (l0, l1) = (p.label(), p.label());
    p.push(Op::Bind(l0));
    p.push(Op::Lea(1, fr(DATA * 8)));
    p.push(Op::SetCi(1));
    p.push(Op::Lea(2, fr(DATA * 8 + 64)));
    p.push(Op::SetK(2));
    p.push(Op::Bind(l1));
    p.push(Op::Ret(0));
    let mut e = emu::Emu::new(&p, l0);
    assert_eq!(e.ci(&env), env.ci);
    // SAFETY: the program only forms addresses.
    let u = unsafe { e.run(&p, &[l1], &env, 100, &mut NoH) };
    assert_eq!(u, Until::Label(l1));
    assert_eq!(e.ci(&env) as usize, env.frame as usize + DATA * 8);
    assert_eq!(e.k(&env) as usize, env.frame as usize + DATA * 8 + 64);
}

#[test]
fn jmpr_to_a_mapped_label_and_out_of_the_program() {
    let mut b = Buf::new();
    let env = b.env();
    let mut p = Program::default();
    let (l0, cell, other) = (p.label(), p.label(), p.label());
    p.push(Op::Bind(l0));
    p.push(Op::Mov(1, 0xABCD_0000));
    p.push(Op::JmpR(1));
    p.push(Op::Ret(9));
    p.push(Op::Bind(other));
    p.push(Op::Ret(8));
    p.push(Op::Bind(cell));
    p.push(Op::St {
        w: W::B8,
        m: fr(8 * MARK),
        src: Src::Imm(7),
    });
    p.push(Op::Ret(0));
    // mapped: continues at the label
    let mut e = emu::Emu::new(&p, l0);
    e.map_target(0x1234, other);
    e.map_target(0xABCD_0000, cell);
    // SAFETY: the frame buffer covers MARK.
    let u = unsafe { e.run(&p, &[], &env, 100, &mut NoH) };
    assert_eq!(u, Until::Stop(Stop::Ret(0)));
    assert_eq!(b.frame[MARK], 7);
    // unmapped: the run stops and reports the address
    let mut e = emu::Emu::new(&p, l0);
    e.map_target(0x1234, other);
    let env = b.env();
    // SAFETY: as above.
    let u = unsafe { e.run(&p, &[], &env, 100, &mut NoH) };
    assert_eq!(u, Until::Stop(Stop::Unsupported));
    assert_eq!(e.jmp_target(), Some(0xABCD_0000));
}

/// Native: `Mov V5, addr(B); JmpR V5` jumps to a second code block; the emulator runs the same program with `B`'s
/// body as a label of the program and the address mapped to it. The pinned registers set before the jump are
/// still in force after it (both sides).
#[cfg(not(miri))]
#[test]
fn diff_jmpr_cell_jump_and_pinned_registers() {
    use rn_jit::mem::CodeArena;
    use rn_jit::x86::{emit, thunk_code, Thunk};
    // the block B body: reads through Ci/K (set before the jump), stores the values and a marker, returns 0
    let body = |p: &mut Program, l: L| {
        p.push(Op::Bind(l));
        p.push(Op::Ld {
            dst: 3,
            w: W::B8,
            m: mk(Base::Ci, 0),
        });
        p.push(Op::Ld {
            dst: 4,
            w: W::B8,
            m: mk(Base::K, 8),
        });
        p.push(Op::St {
            w: W::B8,
            m: fr(8 * VOUT),
            src: Src::V(3),
        });
        p.push(Op::St {
            w: W::B8,
            m: fr(8 * (VOUT + 1)),
            src: Src::V(4),
        });
        p.push(Op::St {
            w: W::B8,
            m: fr(8 * MARK),
            src: Src::Imm(7),
        });
        p.push(Op::Ret(0));
    };
    let mut pb = Program::default();
    let lb = pb.label();
    body(&mut pb, lb);
    let eb = emit(&pb).expect("B assembles");

    let mut arena = CodeArena::new();
    let tc = thunk_code();
    let mut tb = arena.alloc(tc.len(), 16).unwrap();
    arena.writer(&mut tb).unwrap().write_at(0, &tc);
    let mut bb = arena.alloc(eb.code.len(), 16).unwrap();
    arena.writer(&mut bb).unwrap().write_at(0, &eb.code);
    let b_addr = (bb.addr() + eb.label_off[lb as usize].unwrap()) as u64;

    // program A: set Ci/K to frame data, jump to B
    let build = |with_copy: bool| {
        let mut p = Program::default();
        let (l0, lcopy) = (p.label(), p.label());
        p.push(Op::Bind(l0));
        p.push(Op::Lea(1, fr(DATA * 8)));
        p.push(Op::SetCi(1));
        p.push(Op::Lea(2, fr(DATA * 8 + 16)));
        p.push(Op::SetK(2));
        p.push(Op::Mov(5, b_addr));
        p.push(Op::JmpR(5));
        p.push(Op::Ret(9));
        if with_copy {
            body(&mut p, lcopy);
        }
        (p, l0, lcopy)
    };
    let (pa, l0, _) = build(false);
    let ea = emit(&pa).expect("A assembles");
    let mut ab = arena.alloc(ea.code.len(), 16).unwrap();
    arena.writer(&mut ab).unwrap().write_at(0, &ea.code);
    arena.commit().unwrap();
    // SAFETY: `tb` holds the committed thunk.
    let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
    let a_addr = ab.addr() + ea.label_off[l0 as usize].unwrap();

    let (pe, e0, lcopy) = build(true);
    let mut rng = Rng(0xFACE_FEED_1234_5679);
    for _ in 0..200 {
        let (mut bn, mut be) = (Buf::new(), Buf::new());
        for i in 0..6 {
            let w = rng.next();
            bn.frame[DATA + i] = w;
            be.frame[DATA + i] = w;
        }
        let envn = bn.env();
        // SAFETY: A and B are committed blocks; the buffers cover every offset used.
        let code = unsafe {
            thunk(
                envn.thread,
                envn.ci,
                envn.frame,
                envn.cnt,
                a_addr as *const u8,
            )
        };
        let mut emu_ = emu::Emu::new(&pe, e0);
        emu_.map_target(b_addr, lcopy);
        let enve = be.env();
        // SAFETY: as above.
        let u = unsafe { emu_.run(&pe, &[], &enve, 1000, &mut NoH) };
        assert_eq!(u, Until::Stop(Stop::Ret(0)));
        assert_eq!(code, 0);
        let (bnn, ben) = (bn.normalised(), be.normalised());
        assert_eq!(bnn[VOUT], bn.frame[DATA]);
        assert_eq!(bnn[VOUT + 1], bn.frame[DATA + 3]); // K = frame data + 16, offset 8 -> word 3
        assert_eq!(bnn[MARK], 7);
        assert_eq!(&bnn[VOUT..VOUT + 2], &ben[VOUT..VOUT + 2]);
        assert_eq!(bnn[MARK], ben[MARK]);
    }
}

/// Exact encodings, decoded with the independent iced decoder: `Ult`/`Uge` are `jb`/`jae` after the same `cmp`
/// as the signed codes, and `SetCi`/`SetK`/`JmpR` are `mov r13, r` / `mov r14, r` / `jmp r`.
#[cfg(not(miri))]
#[test]
fn oracle_encodings_of_unsigned_cc_and_pinned_ops() {
    use iced_x86::{Decoder, DecoderOptions, Mnemonic as Mn, Register as Rg};
    use rn_jit::x86::emit;
    let decode = |ops: &[Op]| {
        let mut p = Program::default();
        let l = p.label();
        for op in ops {
            p.push(*op);
        }
        p.push(Op::Bind(l));
        let code = emit(&p).expect("assembles").code;
        let mut d = Decoder::with_ip(64, &code, 0, DecoderOptions::NONE);
        let mut v = Vec::new();
        while d.can_decode() {
            let i = d.decode();
            assert!(!i.is_invalid(), "{code:02X?}");
            v.push(i);
        }
        v
    };
    for (cc, mn, sg_cc, sg_mn) in [
        (Cc::Ult, Mn::Jb, Cc::Lt, Mn::Jl),
        (Cc::Uge, Mn::Jae, Cc::Ge, Mn::Jge),
    ] {
        for b in [Src::V(3), Src::Imm(-1)] {
            let ins = decode(&[Op::Br { cc, a: 2, b, to: 0 }]);
            let sg = decode(&[Op::Br {
                cc: sg_cc,
                a: 2,
                b,
                to: 0,
            }]);
            assert_eq!(ins[0].mnemonic(), Mn::Cmp);
            assert_eq!(ins[1].mnemonic(), mn, "{cc:?}");
            assert_eq!(sg[1].mnemonic(), sg_mn);
            // the same cmp bytes as the signed code; only the condition differs
            assert_eq!(ins[0], sg[0]);
        }
        for w in ALL_W {
            let ins = decode(&[Op::BrM {
                cc,
                w,
                m: fr(8),
                imm: 5,
                to: 0,
            }]);
            assert_eq!(ins[0].mnemonic(), Mn::Cmp);
            assert_eq!(ins[1].mnemonic(), mn, "{cc:?} {w:?}");
        }
    }
    // the machine register of V2, taken from the emitter itself (`mov V0, V2`)
    let v2 = decode(&[Op::MovR(0, 2)])[0].op1_register();
    let ins = decode(&[Op::SetCi(2)]);
    assert_eq!((ins.len(), ins[0].mnemonic()), (1, Mn::Mov));
    assert_eq!(
        (ins[0].op0_register(), ins[0].op1_register()),
        (Rg::R13, v2)
    );
    let ins = decode(&[Op::SetK(2)]);
    assert_eq!((ins.len(), ins[0].mnemonic()), (1, Mn::Mov));
    assert_eq!(
        (ins[0].op0_register(), ins[0].op1_register()),
        (Rg::R14, v2)
    );
    let ins = decode(&[Op::JmpR(2)]);
    assert_eq!((ins.len(), ins[0].mnemonic()), (1, Mn::Jmp));
    assert_eq!(ins[0].op0_register(), v2);
}
