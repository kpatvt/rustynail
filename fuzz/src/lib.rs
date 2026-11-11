//! Shared sandbox for the fuzz targets that execute Lua code.
pub mod gen;
use rn_core::alloc::Allocator;
use rn_core::state::{LuaState, State, LUA_MASKCOUNT};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;

/// Allocator with a hard cap (returns null above it, like an out-of-memory condition).
pub struct Capped {
    pub used: usize,
    pub cap: usize,
}

impl Allocator for Capped {
    unsafe fn realloc(&mut self, ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
        let mut sys = rn_core::alloc::SystemAllocator;
        if new > old && self.used - old + new > self.cap {
            return std::ptr::null_mut();
        }
        // SAFETY: forwarded under the allocator contract.
        let r = unsafe { sys.realloc(ptr, old, new) };
        if !r.is_null() || new == 0 {
            self.used = self.used - old + new;
        }
        r
    }
}

/// The count hook: raises an error that only the outermost protected call can catch, so a
/// script cannot `pcall` its way past the budget.
fn budget_hook(st: &mut State, _event: i32, _line: i32) -> LuaResult<()> {
    let s = st.run_error(b"instruction budget exhausted");
    Err(st.throw_base_level(s))
}

fn open_pure_libs(st: &mut State) -> LuaResult<()> {
    use rn_std::{lib_base, lib_coroutine, lib_math, lib_string, lib_table, lib_utf8};
    for (name, f) in [
        (&b"_G"[..], lib_base::open as rn_core::state::NativeFn),
        (b"coroutine", lib_coroutine::open),
        (b"table", lib_table::open),
        (b"string", lib_string::open),
        (b"math", lib_math::open),
        (b"utf8", lib_utf8::open),
    ] {
        st.requiref(name, f, true)?;
        st.pop();
    }
    // LuaJIT-compatible extensions: table.new / table.clear and the string.buffer module (global `buffer`: the sandbox
    // has no `require`); safe for untrusted code, so every execution target covers them
    rn_std::lib_ext::open_ext(st)?;
    let b = st.requiref(b"string.buffer", rn_std::lib_buffer::open, false)?;
    let g = st.globals();
    st.raw_set_str(g, b"buffer", b)?;
    // the `jit` control table (jit.flush inside loops, jit.on/off/status): opt-in library, safe for untrusted code
    rn_std::lib_ext::open_jit(st)?;
    // no file access, output or manual collection control
    let g = st.globals();
    for n in [&b"dofile"[..], b"loadfile", b"print", b"require", b"collectgarbage"] {
        st.raw_set_str(g, n, Value::NIL)?;
    }
    Ok(())
}

/// A sandboxed state. `cfg` bit 0: generational GC; bit 1: GC stress (one step per safepoint).
pub fn new_sandbox(cfg: u8) -> Option<LuaState> {
    if cfg & 2 != 0 {
        std::env::set_var("RUSTYNAIL_GC_STRESS", "2");
    } else {
        std::env::remove_var("RUSTYNAIL_GC_STRESS");
    }
    let mut ls = State::new(Box::new(Capped { used: 0, cap: 64 << 20 }), 0x1234).ok()?;
    ls.set_loader(rn_std::loader);
    open_pure_libs(&mut ls).ok()?;
    if cfg & 1 != 0 {
        ls.gc_change_mode(rn_core::gc::GcKind::GenMinor);
    }
    Some(ls)
}

/// Run `f` (already on the stack at `func`) with the instruction budget.
pub fn run_budgeted(st: &mut State, func: rn_core::state::StkId, nres: i32) -> Status {
    // GC stress makes every safepoint a collection step; a young collection re-traverses every
    // touched old table, so a script growing one big table is quadratic: keep the budget small
    let budget = if std::env::var_os("RUSTYNAIL_GC_STRESS").is_some() {
        30_000
    } else {
        300_000
    };
    st.set_hook(Some(budget_hook), LUA_MASKCOUNT, budget);
    let s = st.pcall(func, nres, 0);
    st.set_hook(None, 0, 0);
    s
}

// ---------------------------------------------------------------------------------------------
// JIT support (feature `jit`): the compiler is registered once per process in `always` mode; a state
// is compiled-or-interpreted according to `rn_jit::set_mode`.

/// Registers the JIT in `always` mode (first call wins; no-op without the `jit` feature).
pub fn jit_init() {
    #[cfg(feature = "jit")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            rn_jit::register_with(rn_jit::Config::for_mode(rn_jit::Mode::Always));
        });
    }
}

/// Switches `ls` between the interpreter and compiled code (no-op without the `jit` feature).
#[allow(unused_variables)]
pub fn jit_mode(ls: &mut LuaState, on: bool) {
    #[cfg(feature = "jit")]
    rn_jit::set_mode(
        ls,
        if on { rn_jit::Mode::Always } else { rn_jit::Mode::Off },
    );
}

/// Tier 1.5 (typed regions) configuration of the compiled run (`jit_diff`, docs/JIT_TIER15.md section 22).
#[derive(Clone, Copy, Debug, Default)]
pub struct T15Cfg {
    /// `false`: `T15=always` (every nest is promoted at its first back-edge); `true`: `T15=on` with a threshold of 4
    /// (the warm-up profile is filled before the promotion).
    pub profiled: bool,
    /// `+exitstorm=N` (0 = off).
    pub exitstorm: u32,
    /// `+shadow`.
    pub shadow: bool,
    /// `+randra=SEED` (0 = off).
    pub randra: u8,
    /// `+verify`.
    pub verify: bool,
}

impl T15Cfg {
    /// Decodes the two configuration bytes of `jit_diff`: `sel` = bits 5-6 of the first byte (0 = no tier 1.5, 1 =
    /// `always`, 2 = profiled, 3 = `always` with `+verify`), `b` = the byte behind it: bits 0-2 exit storm period
    /// index (off, 1, 2, 7, 61, 2, 7, 61), bit 3 shadow, bits 4-7 randra seed (0 = off).
    pub fn decode(sel: u8, b: u8) -> Option<T15Cfg> {
        if sel == 0 {
            return None;
        }
        Some(T15Cfg {
            profiled: sel == 2,
            exitstorm: [0, 1, 2, 7, 61, 2, 7, 61][(b & 7) as usize],
            shadow: b & 8 != 0,
            randra: b >> 4,
            verify: sel == 3,
        })
    }

    /// The `RUSTYNAIL_JIT_OPT` string of this configuration.
    pub fn opt(&self) -> String {
        let mut o = String::from("+force");
        if self.exitstorm > 0 {
            o += &format!(",+exitstorm={}", self.exitstorm);
        }
        if self.shadow {
            o += ",+shadow";
        }
        if self.randra > 0 {
            o += &format!(",+randra={}", self.randra);
        }
        if self.verify {
            o += ",+verify";
        }
        o
    }
}

/// Applies `t` to `ls` (no-op without the `jit` feature).
#[allow(unused_variables)]
pub fn jit_t15(ls: &mut LuaState, t: &T15Cfg) {
    #[cfg(feature = "jit")]
    {
        use rn_jit::t15::runtime::{OptFlags, T15Config, T15Mode};
        let mut c = T15Config::for_mode(if t.profiled { T15Mode::On } else { T15Mode::Always });
        if t.profiled {
            c.hot = 4;
        }
        c.opt = OptFlags::parse(&t.opt());
        rn_jit::set_t15(ls, c);
    }
}

/// Whether the sandbox was built with the JIT.
pub const HAS_JIT: bool = cfg!(feature = "jit");

/// Whether the sandbox was built with the J5 trace compiler (`--features trace`).
pub const HAS_TRACE: bool = cfg!(feature = "trace");

/// J5 trace configuration of the compiled run (`jit_diff`, `run_source`; docs/JIT_TRACE.md 9.8).
#[derive(Clone, Copy, Debug, Default)]
pub struct TraceCfg {
    /// `false`: `TRACE=always`; `true`: `TRACE=on` with back-edge / exit thresholds `hot`.
    pub on: bool,
    /// Thresholds of `TRACE=on` (loop, exit).
    pub hot: (i32, u32),
    /// Index of the option variant (see [`TraceCfg::opt`]).
    pub variant: u8,
    /// `-loop` (the rolled layout).
    pub no_loop: bool,
    /// `-fwd,-abc`.
    pub no_fwd: bool,
    /// `+tracemove`.
    pub tracemove: bool,
    /// `-sink`: allocation sinking off (it is on by default, TR29); the default exercises sinking, exit-time
    /// materialisation, virtual byte accounting and the recorder's watch list under the GC modes of `new_sandbox`.
    pub no_sink: bool,
    /// Seed of `+randra`.
    pub seed: u64,
}

impl TraceCfg {
    /// The `RUSTYNAIL_JIT_TRACE_OPT` string: variant 0 default, 1 `+exitstorm=3`, 2 `+randra`, 3 all passes off,
    /// 4 `+verify`, 5 `+emurun`, 6 `+exitstorm=2,+randra`, 7 `+membudget`; then the flag bits.
    pub fn opt(&self) -> String {
        let mut o: Vec<String> = Vec::new();
        match self.variant & 7 {
            1 => o.push("+exitstorm=3".into()),
            2 => o.push(format!("+randra={}", self.seed % 1000 + 1)),
            3 => o.push("-fold,-cse,-loop,-fwd,-abc".into()),
            4 => o.push("+verify".into()),
            5 => o.push("+emurun".into()),
            6 => o.push(format!("+exitstorm=2,+randra={}", self.seed % 1000 + 1)),
            7 => o.push("+membudget".into()),
            _ => {}
        }
        if self.no_loop {
            o.push("-loop".into());
        }
        if self.no_fwd {
            o.push("-fwd,-abc".into());
        }
        if self.tracemove {
            o.push("+tracemove".into());
        }
        if self.no_sink {
            o.push("-sink".into());
        }
        o.join(",")
    }
}

/// Registers the trace driver once per process (no-op without the `trace` feature).
pub fn trace_init() {
    #[cfg(feature = "trace")]
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            rn_jit::tr::install_driver();
        });
    }
}

/// Applies `t` to `ls`; must run before the chunk is loaded (the dispatch cells are part of the code compiled then).
/// No-op without the `trace` feature.
#[allow(unused_variables)]
pub fn jit_trace(ls: &mut LuaState, t: &TraceCfg) {
    #[cfg(feature = "trace")]
    {
        use rn_jit::tr::cache::{TrConfig, TrMode};
        trace_init();
        let mut c = TrConfig::for_mode(if t.on { TrMode::On } else { TrMode::Always });
        if t.on {
            c.hot_loop = t.hot.0;
            c.hot_exit = t.hot.1;
        }
        let (o, _bad) = rn_jit::tr::TraceOpts::parse_lenient(&t.opt());
        c.opts = o;
        // triage aid: RUSTYNAIL_JIT_VERBOSE=1 prints the trace summary when the state closes
        c.verbose = std::env::var("RUSTYNAIL_JIT_VERBOSE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        rn_jit::rt::set_trace(ls, c);
        rn_jit::tr::attach_driver(ls);
    }
}

thread_local! {
    static LOG: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    static TICKS: std::cell::Cell<i64> = const { std::cell::Cell::new(i64::MAX / 2) };
    /// Number of budget errors raised in this run (varies the grace period, see `tick`).
    static RERAISED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn fmt_value(v: Value, out: &mut Vec<u8>) {
    use rn_core::number::Number;
    if let Some(n) = v.as_number() {
        match n {
            Number::Int(i) => out.extend_from_slice(format!("i{i}").as_bytes()),
            Number::Float(f) if f.is_nan() => {
                out.extend_from_slice(b"f:");
                out.extend_from_slice(rn_core::number::tostring(n).as_bytes());
            }
            Number::Float(f) => out.extend_from_slice(format!("f{:016x}", f.to_bits()).as_bytes()),
        }
    } else if let Some(s) = v.as_string() {
        // addresses (tostring of tables/functions, error messages) differ between runs
        let b = s.as_bytes();
        let mut o = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'0' && b.get(i + 1) == Some(&b'x') {
                let n = b[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
                if n >= 6 {
                    o.extend_from_slice(b"0xADDR");
                    i += 2 + n;
                    continue;
                }
            }
            o.push(b[i]);
            i += 1;
        }
        out.extend_from_slice(format!("s{}:", o.len()).as_bytes());
        out.extend_from_slice(&o);
    } else if let Some(b) = v.as_bool() {
        out.extend_from_slice(if b { b"true" } else { b"false" });
    } else {
        out.extend_from_slice(rn_core::auxlib::typename_of(&v));
    }
}

/// `emit(...)`: appends the arguments (type-tagged, floats bit-exact) to the log.
fn emit(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let mut line = Vec::new();
    for i in 0..n {
        let a = st.arg(i);
        if i > 0 {
            line.push(b'\t');
        }
        fmt_value(a, &mut line);
    }
    line.push(b'\n');
    LOG.with(|l| {
        let mut l = l.borrow_mut();
        if l.len() < (1 << 20) {
            l.extend_from_slice(&line);
        }
    });
    Ok(0)
}

/// Installs the global `emit` (output log of the differential target).
pub fn install_emit(ls: &mut LuaState) -> bool {
    let g = ls.globals();
    ls.raw_set_str(g, b"emit", Value::native(emit)).is_ok()
}

/// `tick(site)`: the cooperative budget of generated programs. Compiled code is never entered while
/// a debug hook is installed (docs/JIT_DESIGN.md, "Hooks active"), so the hook-based budget of
/// [`run_budgeted`] would silently turn a JIT run into an interpreter run. After the allowed number
/// of ticks it raises an error that only the outermost protected call can catch (like the budget
/// hook), naming the site, so a divergence in where the budget runs out is visible.
fn tick(st: &mut State) -> LuaResult<usize> {
    let t = TICKS.with(|c| {
        let v = c.get() - 1;
        c.set(v);
        v
    });
    if t < 0 {
        // Grace for handlers/__close, then raise again: a coroutine boundary can swallow the error,
        // and an unbounded budget afterwards left the program running until the 20 s watchdog.
        // The grace period varies (deterministically, equal in both runs): with a constant 500 a program whose
        // loop body has a period sharing a factor with 501 (e.g. one outer tick + 20 coroutine ticks) re-raised
        // the error only inside the coroutine, which swallows it, and never at the outer loop (found by jit_diff:
        // endless ipairs over a growing table until the watchdog).
        let k = RERAISED.with(|r| {
            r.set(r.get() + 1);
            r.get()
        });
        TICKS.with(|c| c.set(400 + ((k.wrapping_mul(2654435761) >> 7) % 211) as i64));
        let site = st.arg(0).as_int().unwrap_or(-1);
        let s = st.run_error(format!("budget exhausted at site {site}").as_bytes());
        return Err(st.throw_base_level(s));
    }
    Ok(0)
}

// Watchdog for programs that cannot carry ticks (raw source): a thread raises the interpreter's
// process-wide INTERRUPT flag (polled by compiled back-edges and calls too) once a run exceeds its
// deadline, and again every 20 ms (the error is catchable by `pcall`). Runs during which it fired are
// not comparable.
static DEADLINE_US: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FIRED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_us() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_micros() as u64 + 1
}

fn watchdog_start() {
    use std::sync::atomic::Ordering::Relaxed;
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        now_us();
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let d = DEADLINE_US.load(Relaxed);
            if d != 0 && now_us() >= d {
                rn_core::state::request_interrupt();
                FIRED.fetch_add(1, Relaxed);
                DEADLINE_US.store(now_us() + 20_000, Relaxed);
            }
        });
    });
}

/// Arms the watchdog for `ms` milliseconds; returns the fire count to compare with [`disarm`].
fn arm(ms: u64) -> u64 {
    use std::sync::atomic::Ordering::Relaxed;
    watchdog_start();
    rn_core::state::INTERRUPT.store(false, Relaxed);
    let f = FIRED.load(Relaxed);
    DEADLINE_US.store(now_us() + ms * 1000, Relaxed);
    f
}

/// Disarms the watchdog; true if it fired since [`arm`] returned `fired_before`.
fn disarm(fired_before: u64) -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    DEADLINE_US.store(0, Relaxed);
    // the thread may be between its check and the store: let it finish
    std::thread::sleep(std::time::Duration::from_millis(3));
    DEADLINE_US.store(0, Relaxed);
    let fired = FIRED.load(Relaxed) != fired_before;
    rn_core::state::INTERRUPT.store(false, Relaxed);
    fired
}

/// `run_budgeted` for compiled code: no hook (see [`tick`]), a wall-clock watchdog instead.
pub fn run_watchdog(st: &mut State, func: rn_core::state::StkId, nres: i32, ms: u64) -> Status {
    let f = arm(ms);
    let s = st.pcall(func, nres, 0);
    disarm(f);
    s
}

/// Result of one differential run.
#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub status: Status,
    /// Everything `emit`ted, then the final error value or the chunk's results.
    pub log: Vec<u8>,
}

/// How [`run_diff`] runs a program.
#[derive(Clone, Copy, Debug)]
pub struct RunCfg {
    /// `new_sandbox` configuration (bit 0 generational, bit 1 GC stress).
    pub gc: u8,
    /// Compiled (`always`) or interpreter only.
    pub jit: bool,
    /// `Some(n)`: `tick` raises the budget error at the n+1-th call.
    pub ticks: Option<i64>,
    /// `Some(ms)`: the wall-clock watchdog (programs without ticks).
    pub deadline_ms: Option<u64>,
    /// Open the `debug` library.
    pub debug_lib: bool,
    /// Tier 1.5 configuration of the compiled run (`None`: tier 1 only).
    pub t15: Option<T15Cfg>,
    /// Trace compiler configuration of the compiled run (`None`: traces off, J4 behaviour).
    pub trace: Option<TraceCfg>,
}

/// Runs `src` in a fresh sandbox. Returns `None` if the program does not compile or the state
/// cannot be created; the flag is whether the watchdog fired (the outcome is then not comparable).
pub fn run_diff(c: RunCfg, src: &[u8]) -> Option<(Outcome, bool)> {
    jit_init();
    LOG.with(|l| l.borrow_mut().clear());
    TICKS.with(|t| t.set(c.ticks.unwrap_or(i64::MAX / 2)));
    RERAISED.with(|r| r.set(0));
    let mut ls = new_sandbox(c.gc)?;
    jit_mode(&mut ls, c.jit);
    if let Some(t) = c.t15.as_ref().filter(|_| c.jit) {
        jit_t15(&mut ls, t);
    }
    if let Some(t) = c.trace.as_ref().filter(|_| c.jit) {
        jit_trace(&mut ls, t);
    }
    if !install_emit(&mut ls) {
        return None;
    }
    // jit_diff compares the interpreter run with the compiled run: jit.on/off/status would make the two runs differ
    // by design (jit.flush stays: it must be a semantic no-op in both modes)
    {
        let g = ls.globals();
        if let Some(j) = ls.raw_get_str(g, b"jit").ok().and_then(|v| v.as_table()) {
            for n in [&b"on"[..], b"off", b"status"] {
                let _ = ls.raw_set_str(j, n, Value::NIL);
            }
        }
    }
    let g = ls.globals();
    ls.raw_set_str(g, b"tick", Value::native(tick)).ok()?;
    if c.debug_lib {
        ls.requiref(b"debug", rn_std::lib_debug::open, true).ok()?;
        ls.pop();
    }
    if ls.load_chunk(src, b"=fuzz", Some(b"t")) != Status::Ok {
        return None;
    }
    let f = ls.top() - 1;
    let fired = c.deadline_ms.map(arm);
    let status = ls.pcall(f, rn_core::state::LUA_MULTRET, 0);
    let interrupted = fired.is_some_and(disarm);
    let mut log = LOG.with(|l| std::mem::take(&mut *l.borrow_mut()));
    if status == Status::Ok {
        let n = ls.top().0 - f.0;
        log.extend_from_slice(b"RESULT ");
        for i in 0..n {
            let v = ls.get(f + i);
            fmt_value(v, &mut log);
            log.push(b' ');
        }
    } else {
        let v = ls.get(f);
        log.extend_from_slice(format!("ERROR {status:?} ").as_bytes());
        fmt_value(v, &mut log);
    }
    #[cfg(feature = "jit")]
    if c.jit && std::env::var_os("JIT_DIFF_DUMP").is_some() {
        eprintln!("jit stats: {:?}", rn_jit::stats(&mut ls));
    }
    ls.collect(false);
    Some((Outcome { status, log }, interrupted))
}

/// The differential harness of `tests/tr_asm.rs` (emulator world, `compile`, `diff_case`) sliced in by `build.rs`, plus
/// the `trace_ir` entry point (`trace_harness_ext.rs`). Needs `--features trace`.
#[cfg(feature = "trace")]
#[allow(dead_code, unused, clippy::all)]
pub mod trace_harness {
    include!(concat!(env!("OUT_DIR"), "/tr_asm_harness.rs"));
    include!("trace_harness_ext.rs");
}
