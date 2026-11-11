//! Rust emulator of the micro-op IR (`ir`): runs the same [`Program`] the x86-64 backend compiles, on
//! real memory, so the template logic can be checked under Miri (which cannot execute machine code) and
//! compared against the native code.

use crate::ir::{Base, Cc, FOp, Mi, Op, Program, Src, L, M, NV, NX, W};

/// The memory a program runs on (the pinned registers of compiled code).
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// Register 0 of the frame.
    pub frame: *mut u8,
    /// The `CallInfo`.
    pub ci: *mut u8,
    /// The `Thread`.
    pub thread: *mut u8,
    /// The constants.
    pub k: *const u8,
    /// The instruction budget word.
    pub cnt: *mut i32,
}

/// Why emulation stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// `Ret(code)` executed.
    Ret(u32),
    /// The step limit was hit (a template loop that does not terminate).
    OutOfSteps,
    /// An op that only exists in native code (pinned-register writes, indirect jumps, helper calls).
    Unsupported,
}

fn fop(r: &mut [u64; NV], d: u8, s: u8, f: impl Fn(f64, f64) -> f64) {
    let v = f(f64::from_bits(r[d as usize]), f64::from_bits(r[s as usize]));
    r[d as usize] = v.to_bits();
}

/// Result of [`run_until`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// The program stopped for the reason of [`run`].
    Stop(Stop),
    /// Execution reached the `Bind` of this label of the stop list.
    Label(L),
}

/// What the emulator leaves in `V7`/`V8` (`rax`/`rdx`) after `SDivRem` and `UDiv`: natively they hold the
/// quotient/remainder, but no code may depend on that, so the emulator poisons them.
pub const POISON: u64 = 0xDEAD_C10B_BEEF_0000;

const QNAN_BIT: u64 = 0x0008_0000_0000_0000;
/// The "real indefinite" QNaN x86 produces for invalid operations.
const DEFAULT_NAN: u64 = 0xFFF8_0000_0000_0000;

/// One double op with x86 SSE NaN semantics (host-independent, so the result bits are the same natively, in
/// the emulator and under Miri): a NaN first operand wins, else a NaN second operand (either quieted); an
/// invalid operation (`inf - inf`, `0 / 0`, ...) gives the default NaN. `Min`/`Max` return the second operand
/// unless the first is strictly smaller / larger.
fn fbin(op: FOp, a: u64, b: u64) -> u64 {
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    // minsd/maxsd return the second operand untouched (not quieted) unless the first is strictly smaller/larger
    match op {
        FOp::Min => return if x < y { a } else { b },
        FOp::Max => return if x > y { a } else { b },
        _ => {}
    }
    if x.is_nan() {
        return a | QNAN_BIT;
    }
    if y.is_nan() {
        return b | QNAN_BIT;
    }
    let r = match op {
        FOp::Add => x + y,
        FOp::Sub => x - y,
        FOp::Mul => x * y,
        FOp::Div => x / y,
        FOp::Min | FOp::Max => unreachable!(),
    };
    if r.is_nan() {
        DEFAULT_NAN
    } else {
        r.to_bits()
    }
}

/// `roundsd` mode 9 (floor, precision exception suppressed): a NaN is quieted, everything else is `f64::floor`.
fn ffloor(a: u64) -> u64 {
    let x = f64::from_bits(a);
    if x.is_nan() {
        a | QNAN_BIT
    } else {
        x.floor().to_bits()
    }
}

fn fsqrt(a: u64) -> u64 {
    let x = f64::from_bits(a);
    if x.is_nan() {
        a | QNAN_BIT
    } else if x < 0.0 {
        DEFAULT_NAN
    } else {
        x.sqrt().to_bits()
    }
}

/// `cvttsd2si r64, x` followed by the exactness test of `F2IExact`: `(result, exact)`.
fn f2i_exact(a: u64) -> (u64, bool) {
    let x = f64::from_bits(a);
    // the x86 "integer indefinite" for NaN and out-of-range values
    if x.is_nan() || x >= 9_223_372_036_854_775_808.0 || x < -9_223_372_036_854_775_808.0 {
        return (i64::MIN as u64, false);
    }
    let t = x as i64; // in range: truncation toward zero
    (t as u64, (t as f64) == x)
}

fn fcmp(cc: Cc, x: f64, y: f64) -> bool {
    match cc {
        Cc::Eq => x == y,
        Cc::Ne => x != y,
        Cc::Lt => x < y,
        Cc::Le => x <= y,
        Cc::Gt => x > y,
        Cc::Ge => x >= y,
        Cc::Ult | Cc::Uge => unreachable!("unsigned condition on a float compare"),
    }
}

/// Integer compare of two values of `bits` width (sign-extended into `i64`): the signed codes compare as signed
/// numbers, `Ult`/`Uge` as the same bit patterns unsigned.
fn icmp(cc: Cc, x: i64, y: i64, bits: u32) -> bool {
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let (ux, uy) = (x as u64 & mask, y as u64 & mask);
    match cc {
        Cc::Eq => x == y,
        Cc::Ne => x != y,
        Cc::Lt => x < y,
        Cc::Le => x <= y,
        Cc::Gt => x > y,
        Cc::Ge => x >= y,
        Cc::Ult => ux < uy,
        Cc::Uge => ux >= uy,
    }
}

/// # Safety
/// `p` is valid for reading `w` bytes.
unsafe fn load(p: *mut u8, w: W) -> u64 {
    // SAFETY: per the caller.
    unsafe {
        match w {
            W::B1 => u64::from(p.read()),
            W::B4 => u64::from(p.cast::<u32>().read_unaligned()),
            W::B8 => p.cast::<u64>().read_unaligned(),
        }
    }
}

/// # Safety
/// `p` is valid for writing `w` bytes.
unsafe fn store(p: *mut u8, w: W, v: u64) {
    // SAFETY: per the caller.
    unsafe {
        match w {
            W::B1 => p.write(v as u8),
            W::B4 => p.cast::<u32>().write_unaligned(v as u32),
            W::B8 => p.cast::<u64>().write_unaligned(v),
        }
    }
}

/// `BrM` condition: both sides signed at the width of the access, the immediate truncated to it.
fn brm_taken(cc: Cc, w: W, v: u64, imm: i32) -> bool {
    match w {
        W::B1 => icmp(cc, i64::from(v as u8 as i8), i64::from(imm as u8 as i8), 8),
        W::B4 => icmp(cc, i64::from(v as u32 as i32), i64::from(imm), 32),
        W::B8 => icmp(cc, v as i64, i64::from(imm), 64),
    }
}

/// The label table of `prog` (index of the `Bind` of every label), for [`Emu::with_labels`].
pub fn label_table_of(prog: &Program) -> Vec<usize> {
    label_table(prog)
}

fn label_table(prog: &Program) -> Vec<usize> {
    let mut t = vec![usize::MAX; prog.nlabels as usize];
    for (i, op) in prog.ops.iter().enumerate() {
        if let Op::Bind(l) = op {
            t[*l as usize] = i;
        }
    }
    t
}

/// Runs `prog` from label `start` until it returns, or `max_steps` ops were executed.
///
/// # Safety
/// Every address a memory operand computes must be valid for the access (the same contract as the
/// generated machine code): `env` pointers must point to live buffers large enough for the offsets the
/// program uses, and `Mov`ed absolute addresses used as bases must be live.
pub unsafe fn run(prog: &Program, start: L, env: &Env, max_steps: u64) -> Stop {
    // SAFETY: the contract of `run_until` is the contract of `run`.
    match unsafe { run_until(prog, start, &[], env, max_steps) } {
        Until::Stop(s) => s,
        Until::Label(_) => unreachable!("empty stop list"),
    }
}

/// Like [`run`], but returns [`Until::Label`] as soon as execution reaches the `Bind` of a label in `stop`
/// (the `Bind` of `start` itself, executed first, never stops). Used by the shadow verification of tier 1.5
/// (docs/JIT_TIER15.md 7.4): run the tier-1 code of one block up to the next block head.
///
/// # Safety
/// As for [`run`].
pub unsafe fn run_until(prog: &Program, start: L, stop: &[L], env: &Env, max_steps: u64) -> Until {
    let mut emu = Emu::new(prog, start);
    // SAFETY: the contract of `run_until` is the contract of `Emu::run`; the program calls no helper.
    unsafe { emu.run(prog, stop, env, max_steps, &mut NoHelpers) }
}

/// [`run_until`] on a precomputed label table (see [`Emu::with_labels`]); a helper call stops the emulation as
/// [`Stop::Unsupported`].
///
/// # Safety
/// As for [`run`].
pub unsafe fn run_until_shared(
    prog: &Program,
    labels: &std::sync::Arc<Vec<usize>>,
    start: L,
    stop: &[L],
    env: &Env,
    max_steps: u64,
) -> Until {
    let mut emu = Emu::with_labels(std::sync::Arc::clone(labels), start);
    // SAFETY: the contract of this function is the contract of `Emu::run`; the program calls no helper.
    unsafe { emu.run(prog, stop, env, max_steps, &mut NoHelpers) }
}

/// The helper-call model of an emulation (`CallH`, `ReloadFrame`).
///
/// A native helper clobbers every virtual register and every `X` register (the emulator poisons them), may write
/// anything it can reach through `env` (the Lua stack, the `CallInfo`, the thread) and may *move the Lua stack*: it
/// then returns the new frame base in [`HelperOut::new_frame`]. Compiled code must not touch the frame again before a
/// `ReloadFrame` (the emulator makes the old base unusable until then, so a missing reload is caught).
pub trait Helpers {
    /// Runs the helper `f` (address from the `CallH`) with the `a`, `b` arguments; the result (`V0`) is
    /// `HelperOut::result`. `frame` is the current frame base.
    ///
    /// # Safety
    /// The helper may dereference the `env` pointers and `frame` (valid per the contract of [`Emu::run`]).
    unsafe fn call(&mut self, f: u64, a: u32, b: u32, env: &Env, frame: *mut u8) -> HelperOut;
    /// Does this model implement helpers at all (else a call stops the emulation as `Unsupported`)?
    fn supported(&self) -> bool {
        true
    }
}

/// What a helper call did.
#[derive(Clone, Copy, Debug)]
pub struct HelperOut {
    /// The helper's return value (`V0`).
    pub result: u32,
    /// The Lua stack moved: the new base of register 0 (valid after the next `ReloadFrame`).
    pub new_frame: Option<*mut u8>,
}

/// Helpers for programs that never call one (a call stops the emulation as [`Stop::Unsupported`]).
struct NoHelpers;
impl Helpers for NoHelpers {
    fn supported(&self) -> bool {
        false
    }
    unsafe fn call(&mut self, _f: u64, _a: u32, _b: u32, _env: &Env, _frame: *mut u8) -> HelperOut {
        HelperOut {
            result: u32::MAX,
            new_frame: None,
        }
    }
}

/// A frame base nothing may be accessed through (after a helper that moved the stack, until `ReloadFrame`).
const STALE_FRAME: usize = 0xDEAD_0000;

/// An emulation in progress: registers, program counter and the frame base. [`Emu::run`] can be called again after
/// it returned [`Until::Label`] and continues right behind the `Bind` it stopped at.
pub struct Emu {
    /// The virtual registers.
    pub r: [u64; NV],
    /// The `X` registers.
    pub x: [u64; NX],
    labels: std::sync::Arc<Vec<usize>>,
    pc: usize,
    steps: u64,
    frame: Option<*mut u8>,
    pending: Option<*mut u8>,
    /// Pinned `r13` after a `SetCi` (else `env.ci`).
    ci: Option<*mut u8>,
    /// Pinned `r14` after a `SetK` (else `env.k`).
    k: Option<*const u8>,
    /// Native addresses a `JmpR` may reach inside the program (see [`Emu::map_target`]).
    targets: Vec<(u64, L)>,
    /// The operand of the last `JmpR` that left the program (`Stop::Unsupported`).
    jmp_target: Option<u64>,
}

impl Emu {
    /// An emulation starting at `start`.
    pub fn new(prog: &Program, start: L) -> Emu {
        Emu::with_labels(std::sync::Arc::new(label_table(prog)), start)
    }

    /// An emulation starting at `start` with a precomputed label table ([`label_table_of`]): repeated runs of the
    /// same program (shadow verification) do not pay for the table every time.
    pub fn with_labels(labels: std::sync::Arc<Vec<usize>>, start: L) -> Emu {
        let pc = labels[start as usize];
        Emu {
            r: [0; NV],
            x: [0; NX],
            labels,
            pc,
            steps: 0,
            frame: None,
            pending: None,
            ci: None,
            k: None,
            targets: Vec::new(),
            jmp_target: None,
        }
    }

    /// Declares that the native address `addr` is the position of label `l`: a `JmpR` to `addr` continues at the
    /// `Bind` of `l` (a cell jump inside the program). A `JmpR` to any other address stops the emulation as
    /// [`Stop::Unsupported`] with the address in [`Emu::jmp_target`] (the program left the emulated block).
    pub fn map_target(&mut self, addr: u64, l: L) {
        self.targets.push((addr, l));
    }

    /// The address of the `JmpR` that stopped the last run as `Unsupported`, if that was the reason.
    pub fn jmp_target(&self) -> Option<u64> {
        self.jmp_target
    }

    /// The pinned `CallInfo` pointer (`r13`): the one of the `env` until a `SetCi` changed it.
    pub fn ci(&self, env: &Env) -> *mut u8 {
        self.ci.unwrap_or(env.ci)
    }

    /// The pinned constants pointer (`r14`): the one of the `env` until a `SetK` changed it.
    pub fn k(&self, env: &Env) -> *const u8 {
        self.k.unwrap_or(env.k)
    }

    /// The current frame base (the base of the moved stack after a helper moved it and `ReloadFrame` ran).
    pub fn frame(&self, env: &Env) -> *mut u8 {
        self.frame.unwrap_or(env.frame)
    }

    /// Runs until `Ret`, the step limit (counted over all calls), or a `Bind` of a label in `stop` (the `Bind` of the
    /// start label, executed first, never stops).
    ///
    /// # Safety
    /// As for [`run`]; `helpers` must obey the contract of [`Helpers`].
    pub unsafe fn run(
        &mut self,
        prog: &Program,
        stop: &[L],
        env: &Env,
        max_steps: u64,
        helpers: &mut dyn Helpers,
    ) -> Until {
        let labels = std::sync::Arc::clone(&self.labels);
        let mut r = self.r;
        let mut x = self.x;
        let mut pc = self.pc;
        let mut steps = self.steps;
        let frame = std::cell::Cell::new(self.frame.unwrap_or(env.frame));
        let mut pending = self.pending;
        let ci = std::cell::Cell::new(self.ci.unwrap_or(env.ci));
        let kp = std::cell::Cell::new(self.k.unwrap_or(env.k));
        // leaves the loop with `$ret`, writing the state back
        macro_rules! leave {
            ($ret:expr) => {{
                self.r = r;
                self.x = x;
                self.pc = pc;
                self.steps = steps;
                self.frame = Some(frame.get());
                self.pending = pending;
                self.ci = Some(ci.get());
                self.k = Some(kp.get());
                return $ret;
            }};
        }
        let addr = |r: &[u64; NV], m: &M| -> *mut u8 {
            let b: *mut u8 = match m.base {
                Base::Frame => frame.get(),
                Base::Ci => ci.get(),
                Base::Thread => env.thread,
                Base::K => kp.get().cast_mut(),
                Base::Cnt => env.cnt.cast(),
                Base::Reg(v) => r[v as usize] as *mut u8,
            };
            b.wrapping_offset(m.off as isize)
        };
        let addri = |r: &[u64; NV], m: &Mi| -> *mut u8 {
            let a = r[m.base as usize]
                .wrapping_add(r[m.idx as usize].wrapping_mul(u64::from(m.scale)))
                .wrapping_add(m.off as i64 as u64);
            a as usize as *mut u8
        };
        let sv = |r: &[u64; NV], s: Src| -> u64 {
            match s {
                Src::V(v) => r[v as usize],
                Src::Imm(i) => i as i64 as u64,
            }
        };
        loop {
            steps += 1;
            if steps > max_steps {
                leave!(Until::Stop(Stop::OutOfSteps));
            }
            let op = prog.ops[pc];
            pc += 1;
            match op {
                Op::Align(_) | Op::Pad(_) => {}
                Op::Bind(l) => {
                    if steps > 1 && stop.contains(&l) {
                        leave!(Until::Label(l));
                    }
                }
                Op::Mov(v, i) => r[v as usize] = i,
                Op::MovR(d, s) => r[d as usize] = r[s as usize],
                Op::Ld { dst, w, m } => {
                    // SAFETY: per the function contract.
                    r[dst as usize] = unsafe { load(addr(&r, &m), w) };
                }
                Op::St { w, m, src } => {
                    let v = sv(&r, src);
                    // SAFETY: per the function contract.
                    unsafe { store(addr(&r, &m), w, v) };
                }
                Op::LdI { dst, w, m } => {
                    // SAFETY: per the function contract.
                    r[dst as usize] = unsafe { load(addri(&r, &m), w) };
                }
                Op::StI { w, m, src } => {
                    let v = sv(&r, src);
                    // SAFETY: per the function contract.
                    unsafe { store(addri(&r, &m), w, v) };
                }
                Op::LdF { dst, m } => {
                    // SAFETY: per the function contract.
                    x[dst as usize] = unsafe { load(addr(&r, &m), W::B8) };
                }
                Op::StF { m, src } => {
                    // SAFETY: per the function contract.
                    unsafe { store(addr(&r, &m), W::B8, x[src as usize]) };
                }
                Op::LdFi { dst, m } => {
                    // SAFETY: per the function contract.
                    x[dst as usize] = unsafe { load(addri(&r, &m), W::B8) };
                }
                Op::StFi { m, src } => {
                    // SAFETY: per the function contract.
                    unsafe { store(addri(&r, &m), W::B8, x[src as usize]) };
                }
                Op::MovF(d, s) => x[d as usize] = x[s as usize],
                Op::FBin { op, dst, src } => {
                    x[dst as usize] = fbin(op, x[dst as usize], x[src as usize]);
                }
                Op::FBinM { op, dst, m } => {
                    // SAFETY: per the function contract.
                    let b = unsafe { load(addr(&r, &m), W::B8) };
                    x[dst as usize] = fbin(op, x[dst as usize], b);
                }
                Op::FSqrt(d, s) => x[d as usize] = fsqrt(x[s as usize]),
                Op::FFloor(d, s) => x[d as usize] = ffloor(x[s as usize]),
                Op::FNeg(d) => {
                    x[if d == 0 { 1 } else { 0 }] = 0x8000_0000_0000_0000;
                    r[7] = 0x8000_0000_0000_0000;
                    x[d as usize] ^= 0x8000_0000_0000_0000;
                }
                Op::FAbs(d) => {
                    x[if d == 0 { 1 } else { 0 }] = 0x7FFF_FFFF_FFFF_FFFF;
                    r[7] = 0x7FFF_FFFF_FFFF_FFFF;
                    x[d as usize] &= 0x7FFF_FFFF_FFFF_FFFF;
                }
                Op::GtoF(d, v) => x[d as usize] = r[v as usize],
                Op::FtoG(v, s) => r[v as usize] = x[s as usize],
                Op::I2Fx(d, v) => x[d as usize] = ((r[v as usize] as i64) as f64).to_bits(),
                Op::F2IFloor { dst, src, to } => {
                    let (v, _) = f2i_exact(x[src as usize]);
                    r[dst as usize] = v;
                    if v as i64 == i64::MIN {
                        pc = labels[to as usize];
                    }
                }
                Op::F2IExact { dst, src, to } => {
                    let (v, exact) = f2i_exact(x[src as usize]);
                    r[dst as usize] = v;
                    x[if src == 0 { 1 } else { 0 }] = (v as i64 as f64).to_bits();
                    if !exact {
                        pc = labels[to as usize];
                    }
                }
                Op::FBrX { cc, a, b, to } => {
                    if fcmp(
                        cc,
                        f64::from_bits(x[a as usize]),
                        f64::from_bits(x[b as usize]),
                    ) {
                        pc = labels[to as usize];
                    }
                }
                Op::BrM { cc, w, m, imm, to } => {
                    // SAFETY: per the function contract.
                    let v = unsafe { load(addr(&r, &m), w) };
                    if brm_taken(cc, w, v, imm) {
                        pc = labels[to as usize];
                    }
                }
                Op::BrM16 { cc, m, imm, to } => {
                    // SAFETY: per the function contract.
                    let v = unsafe { addr(&r, &m).cast::<u16>().read_unaligned() };
                    if (v == imm) == (cc == Cc::Eq) {
                        pc = labels[to as usize];
                    }
                }
                Op::BrMR { cc, m, src, to } => {
                    // SAFETY: per the function contract.
                    let v = unsafe { addr(&r, &m).cast::<u64>().read_unaligned() };
                    if (v == r[src as usize]) == (cc == Cc::Eq) {
                        pc = labels[to as usize];
                    }
                }
                Op::AddM { w, m, imm } => {
                    // SAFETY: per the function contract.
                    unsafe {
                        if w == W::B4 {
                            let p = addr(&r, &m) as *mut u32;
                            p.write_unaligned(p.read_unaligned().wrapping_add(imm as u32));
                        } else {
                            let p = addr(&r, &m) as *mut u64;
                            p.write_unaligned(p.read_unaligned().wrapping_add(imm as i64 as u64));
                        }
                    }
                }
                Op::BrMi { cc, w, m, imm, to } => {
                    // SAFETY: per the function contract.
                    let v = unsafe { load(addri(&r, &m), w) };
                    if brm_taken(cc, w, v, imm) {
                        pc = labels[to as usize];
                    }
                }
                Op::Lea(d, m) => r[d as usize] = addr(&r, &m) as usize as u64,
                Op::Add(d, s) => r[d as usize] = r[d as usize].wrapping_add(sv(&r, s)),
                Op::Sub(d, s) => r[d as usize] = r[d as usize].wrapping_sub(sv(&r, s)),
                Op::And(d, s) => r[d as usize] &= sv(&r, s),
                Op::Mul(d, s) => r[d as usize] = r[d as usize].wrapping_mul(sv(&r, s)),
                Op::Or(d, s) => r[d as usize] |= sv(&r, s),
                Op::Xor(d, s) => r[d as usize] ^= sv(&r, s),
                Op::Neg(d) => r[d as usize] = r[d as usize].wrapping_neg(),
                Op::Not(d) => r[d as usize] = !r[d as usize],
                Op::ShlI(d, n) => r[d as usize] <<= n,
                Op::ShrI(d, n) => r[d as usize] >>= n,
                Op::SarI(d, n) => r[d as usize] = ((r[d as usize] as i64) >> n) as u64,
                Op::ShlV(d) => r[d as usize] <<= r[6] & 63,
                Op::ShrV(d) => r[d as usize] >>= r[6] & 63,
                Op::SDivRem { q, r: rem, a, b } => {
                    let (p, d) = (r[a as usize] as i64, r[b as usize] as i64);
                    r[q as usize] = p.wrapping_div(d) as u64;
                    r[rem as usize] = p.wrapping_rem(d) as u64;
                    r[7] = POISON;
                    r[8] = POISON;
                }
                Op::MulHiU { dst, a, k } => {
                    let hi = ((u128::from(r[a as usize]) * u128::from(k)) >> 64) as u64;
                    r[7] = POISON;
                    r[8] = POISON;
                    r[dst as usize] = hi;
                }
                Op::FAdd(d, s) => fop(&mut r, d, s, |x, y| x + y),
                Op::FSub(d, s) => fop(&mut r, d, s, |x, y| x - y),
                Op::FMul(d, s) => fop(&mut r, d, s, |x, y| x * y),
                Op::FDiv(d, s) => fop(&mut r, d, s, |x, y| x / y),
                Op::I2F(d) => r[d as usize] = ((r[d as usize] as i64) as f64).to_bits(),
                Op::FBr { cc, a, b, to } => {
                    let (p, q) = (f64::from_bits(r[a as usize]), f64::from_bits(r[b as usize]));
                    if fcmp(cc, p, q) {
                        pc = labels[to as usize];
                    }
                }
                Op::UDiv { dst, a, b } => {
                    r[dst as usize] = r[a as usize] / r[b as usize];
                    r[7] = POISON;
                    r[8] = POISON;
                }
                Op::Br { cc, a, b, to } => {
                    if icmp(cc, r[a as usize] as i64, sv(&r, b) as i64, 64) {
                        pc = labels[to as usize];
                    }
                }
                Op::SubJl { d, imm, to } => {
                    let v = r[d as usize].wrapping_sub(i64::from(imm) as u64);
                    r[d as usize] = v;
                    if (v as i64) < 0 {
                        pc = labels[to as usize];
                    }
                }
                Op::Jmp(l) => pc = labels[l as usize],
                Op::CountSub { n, exit } => {
                    // SAFETY: per the function contract.
                    let v = unsafe { env.cnt.read_unaligned() }.wrapping_sub(n as i32);
                    // SAFETY: as above.
                    unsafe { env.cnt.write_unaligned(v) };
                    if v < 0 {
                        pc = labels[exit as usize];
                    }
                }
                Op::Ret(c) => leave!(Until::Stop(Stop::Ret(c))),
                Op::Shl(d, n) => r[d as usize] <<= n & 63,
                Op::CallH { f, a, b } => {
                    if !helpers.supported() {
                        leave!(Until::Stop(Stop::Unsupported));
                    }
                    // SAFETY: per the function contract.
                    // (N2b: the pinned registers a `SetCi` / `SetFrame` / `SetK` changed are the helper's `ci` / frame / constants: an exit after a `RETF` hands the caller's frame to the handler)
                    let live = Env {
                        frame: frame.get(),
                        ci: ci.get(),
                        k: kp.get(),
                        ..*env
                    };
                    let out = unsafe { helpers.call(f, a, b, &live, frame.get()) };
                    let keep = [r[9], r[10]]; // V9 / V10 (rbp, r14): callee-saved, survive the call
                    r = [POISON; NV];
                    r[9] = keep[0];
                    r[10] = keep[1];
                    x = [POISON; NX];
                    r[0] = u64::from(out.result);
                    if let Some(nf) = out.new_frame {
                        pending = Some(nf);
                        frame.set(STALE_FRAME as *mut u8);
                    }
                }
                Op::ReloadFrame { .. } => {
                    if let Some(nf) = pending.take() {
                        frame.set(nf);
                    } else {
                        // (nothing moved; the base re-derived from the thread is the one we have)
                    }
                    r[7] = POISON;
                    r[8] = POISON;
                }
                Op::SetCi(v) => ci.set(r[v as usize] as usize as *mut u8),
                Op::SetK(v) => kp.set(r[v as usize] as usize as *const u8),
                Op::ReloadK => {}
                Op::JmpR(v) => {
                    let t = r[v as usize];
                    if let Some(&(_, l)) = self.targets.iter().find(|(a, _)| *a == t) {
                        pc = labels[l as usize];
                    } else {
                        self.jmp_target = Some(t);
                        leave!(Until::Stop(Stop::Unsupported));
                    }
                }
                // N2b: the frame base moves (a native pop/push): later accesses through `Base::Frame` use the new base. A pending stack
                // move (a helper moved the Lua stack and no `ReloadFrame` ran yet) is dropped: the new base was computed by the program
                Op::SetFrame(v) => {
                    pending = None;
                    frame.set(r[v as usize] as usize as *mut u8);
                }
            }
        }
    }
}
