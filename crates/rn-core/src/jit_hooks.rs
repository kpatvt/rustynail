//! Interpreter-side JIT hooks: hotness counters, the frame-entry flag and the registry through which the
//! compiler crate (`rn-jit`, which depends on this crate and not the other way round) plugs in. Only
//! compiled under the `jit` feature. See docs/JIT_HOOK_COST.md (measurement of the hooks) and
//! docs/JIT_DESIGN.md (protocol).
//!
//! * Counter A (`jit` alone): `Proto.hot` counts down; function starts weigh `HOT_ENTRY_WEIGHT`,
//!   loop back-edges 1; reaching 0 calls [`jit_hot`].
//! * Counter B (`jit_counter_b`): one sampling tick per thread, decremented at back-edges (and at frame
//!   starts in placements (c)/(d)); every [`tick_period`] events [`jit_hot_loop`] fires.
//! * Placement (e) (`jit_check_e`): the entry counter is `Proto.hot`, decremented in `pre_call*`; a
//!   compiled prototype keeps it parked at a value <= 0, so every call reaches [`jit_hot_call`].
//! * The real entry (J1a): [`jit_hot_call`] / [`jit_hot_loop`] ask the registered compiler ([`install`]); the
//!   frame is then flagged with [`TRAP_JIT`] and the interpreter's `'frame` head calls [`enter`].
//!   Without a registered compiler the hooks only count (the measurement prototype behaviour).

use crate::state::{Ci, State};
use crate::status::Status;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

pub const HOT_THRESHOLD: i32 = 1000;
pub const HOT_ENTRY_WEIGHT: i32 = 5;
pub const TICK_PERIOD: i32 = 1000;
/// Default calls before a function is compiled.
pub const DEFAULT_ENTRY_CALLS: i32 = HOT_THRESHOLD / HOT_ENTRY_WEIGHT;
/// Value at which a compiled proto's counter is parked in placement (e).
pub const COMPILED_PARK: i32 = -(1 << 30);

/// Variant G: global sampling tick.
#[cfg(feature = "jit_counter_g")]
pub(crate) static TICK: std::sync::atomic::AtomicI32 =
    std::sync::atomic::AtomicI32::new(TICK_PERIOD);

/// Value a counter is reset to after [`jit_hot`] fired. The default parks it far away: the
/// prototype never compiles, so re-firing every 200 calls / 1000 edges would only measure the stub
/// (a real implementation compiles, or marks the proto "do not compile", and stops counting).
/// Tests set it to [`HOT_THRESHOLD`] with [`set_reset_value`] to observe repeated firing.
pub const PARK_HIGH: i32 = i32::MAX / 2;
static RESET: AtomicI32 = AtomicI32::new(PARK_HIGH);

/// Set the value counters are reset to after firing (see [`PARK_HIGH`]); returns the old value.
pub fn set_reset_value(v: i32) -> i32 {
    RESET.swap(v, Ordering::Relaxed)
}

static HOT_FIRED: AtomicU64 = AtomicU64::new(0);

/// Statistic only (racy by design: a plain load + store, not a `lock xadd`, which cost ~25% of an
/// interpreter-run call into compiled code).
#[inline(always)]
fn bump_fired() {
    HOT_FIRED.store(
        HOT_FIRED.load(Ordering::Relaxed).wrapping_add(1),
        Ordering::Relaxed,
    );
}
static ENTER_CALLS: AtomicU64 = AtomicU64::new(0);
static ENTER_ENABLED: AtomicBool = AtomicBool::new(false);

/// Number of times a hotness hook has fired (all threads).
pub fn hot_fired() -> u64 {
    HOT_FIRED.load(Ordering::Relaxed)
}

/// Number of times [`jit_try_enter`] was reached (placements (c)/(d) with a non-null `Proto.jit`).
pub fn enter_calls() -> u64 {
    ENTER_CALLS.load(Ordering::Relaxed)
}

/// The counter crossed its threshold. Without a registered compiler the prototype only resets the
/// counter(s) (see [`PARK_HIGH`]) and bumps the global statistic. `proto` is `None` for variant B/G.
#[cold]
#[inline(never)]
pub fn jit_hot(st: &mut State, proto: Option<&crate::func::Proto>) {
    bump_fired();
    #[cfg(feature = "jit_counter_b")]
    st.jit_tick_reset();
    #[cfg(not(feature = "jit_counter_b"))]
    let _ = st;
    if let Some(p) = proto {
        p.hot.set(RESET.load(Ordering::Relaxed));
    }
}

/// Placement (e): the merged counter of `proto` reached 0 (or `proto` is compiled and its counter is
/// parked at a value <= 0 so that every call lands here).
///
/// With a compiler installed: compiles on the first call, then flags the fresh frame `ci` with
/// [`TRAP_JIT`]; the interpreter's `'frame` head consumes the flag and enters the compiled code. The
/// counter is parked at [`COMPILED_PARK`] (compiled) or [`PARK_HIGH`] (will never compile).
#[cold]
#[inline(never)]
pub fn jit_hot_call(st: &mut State, ci: Ci, proto: &crate::func::Proto) {
    if let Some(vt) = VTABLE.get() {
        bump_fired();
        match (vt.hot_call)(st, ci, proto) {
            HotCall::Enter => {
                proto.hot.set(COMPILED_PARK);
                if st.hook_mask() == 0 {
                    ci.w().trap = TRAP_JIT;
                }
            }
            HotCall::NoJit => proto.hot.set(PARK_HIGH),
            HotCall::Retry => proto.hot.set(initial_hot()),
            HotCall::Skip => {}
        }
        return;
    }
    if !proto.jit.get().is_null() {
        proto.hot.set(COMPILED_PARK);
        jit_try_enter(st, ci);
    } else {
        jit_hot(st, Some(proto));
    }
}

static ALLOC_REFUSED: AtomicBool = AtomicBool::new(false);

/// N3b: a trace helper refused an allocation (the trace leaves at it).
#[inline]
pub fn note_alloc_refusal() {
    ALLOC_REFUSED.store(true, Ordering::Relaxed);
}

/// N3b: whether a helper refused an allocation since the last call (and forgets it).
#[inline]
pub fn take_alloc_refusal() -> bool {
    ALLOC_REFUSED.swap(false, Ordering::Relaxed)
}

/// N3b: arms the sampling tick so that it fires at the next loop back-edge (no-op without a tick counter).
pub fn arm_tick_next(st: &mut State) {
    #[cfg(any(feature = "jit_counter_b", feature = "jit_counter_g"))]
    st.jit_tick_arm(1);
    #[cfg(not(any(feature = "jit_counter_b", feature = "jit_counter_g")))]
    let _ = st;
}

/// R1 (`rearm`): a stay of compiled code handed a frame to the interpreter: the next `span` loop back-edges of the interpreter fire a
/// *rearmed* tick for `key` (the prototype the frame belongs to; `span` counts in units, see `rn-jit`'s `REARM_UNIT`) (it restores the formation countdown instead of resetting it and reaches the compiler as `rearmed != 0`).
pub fn rearm_tick(st: &mut State, span: u32, key: usize) {
    #[cfg(all(feature = "jit_counter_b", not(feature = "jit_counter_g")))]
    st.jit_tick_rearm(span, key);
    #[cfg(not(all(feature = "jit_counter_b", not(feature = "jit_counter_g"))))]
    let _ = (st, span, key);
}

/// R1: the key the pending (or last) [`rearm_tick`] was made for.
pub fn rearm_key(st: &State) -> usize {
    #[cfg(feature = "jit_counter_b")]
    return st.jit_rearm_key();
    #[cfg(not(feature = "jit_counter_b"))]
    {
        let _ = st;
        0
    }
}

/// R1: the sampling countdown of the running thread (back-edges until the next tick; 0 without a tick counter).
pub fn tick_left(st: &State) -> i32 {
    #[cfg(feature = "jit_counter_b")]
    return st.jit_tick_left();
    #[cfg(not(feature = "jit_counter_b"))]
    {
        let _ = st;
        0
    }
}

/// R1 mutant support: resets the sampling countdown to a whole period.
pub fn tick_reset(st: &mut State) {
    #[cfg(feature = "jit_counter_b")]
    st.jit_tick_reset();
    #[cfg(not(feature = "jit_counter_b"))]
    let _ = st;
}

/// Counter B fired at a loop back-edge of the interpreter frame `ci`, whose next instruction is `pc`.
/// Returns true when the frame can be continued in compiled code at `pc` (OSR): the caller then stores
/// `pc` in `ci.savedpc`, flags the frame with [`TRAP_JIT`] and restarts it.
#[cold]
#[inline(never)]
pub fn jit_hot_loop(st: &mut State, ci: Ci, pc: *const u32) -> bool {
    match VTABLE.get() {
        Some(vt) => {
            bump_fired();
            #[allow(unused_mut)]
            let mut rearmed = 0u32;
            #[cfg(feature = "jit_counter_b")]
            {
                rearmed = st.jit_tick_take_rearm();
                if rearmed == 0 {
                    st.jit_tick_reset();
                }
            }
            ci.r().trap == 0 && st.hook_mask() == 0 && (vt.hot_loop)(st, ci, pc, rearmed)
        }
        None => {
            jit_hot(st, None);
            false
        }
    }
}

/// Counter variant C: the loop budget of the activation `ci` ran out.
#[cfg(feature = "jit_counter_c")]
#[cold]
#[inline(never)]
pub fn jit_hot_loop_c(st: &mut State, ci: Ci) {
    let _ = st;
    bump_fired();
    ci.w().hot = RESET.load(Ordering::Relaxed);
}

/// E1 hit case of placements (c)/(d): the frame's proto has compiled code. The prototype never enters
/// (returns false); the real entry goes through [`TRAP_JIT`] and [`enter`].
#[cold]
#[inline(never)]
pub fn jit_try_enter(st: &mut State, ci: Ci) -> bool {
    let _ = (st, ci);
    ENTER_CALLS.fetch_add(1, Ordering::Relaxed);
    ENTER_ENABLED.load(Ordering::Relaxed)
}

// ---- the registered compiler (crate `rn-jit`) -----------------------------------------------------

/// Value of `CallInfo.trap` that means "enter compiled code for this frame at `savedpc`" (J1a). Set by
/// [`jit_hot_call`] and by the loop hook; consumed (reset to 0) by the interpreter's `'frame` head, which
/// ignores it (like any other trap) when hooks or an interrupt are pending.
pub const TRAP_JIT: i32 = 2;

/// Why compiled code returned to the interpreter (the thunk's `eax`; docs/JIT_DESIGN.md section 2).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JitExit {
    /// `ci.savedpc` holds the next instruction to run; continue the same frame in the interpreter.
    Continue = 0,
    /// Compiled code popped or pushed Lua frames: continue with `st.ci()`.
    FrameChanged = 1,
    /// The `CIST_FRESH` frame returned: end the interpreter loop (`return Ok(None)`).
    Returned = 2,
    /// A helper reported an error: `return Err(take_error(st))`.
    Error = 3,
}

thread_local! {
    /// BUNDLE-1: the tail call being prepared comes from compiled code (the tier-1 `TAILCALL` helper), which already declined its
    /// inline path: entering a compiled callee again would only bounce between the interpreter and compiled code.
    pub static TAIL_FROM_JIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Verdict of the compiler's entry hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotCall {
    /// Compiled code exists: enter it.
    Enter,
    /// The prototype will not be compiled (disabled, failed, unsupported): stop counting.
    NoJit,
    /// Compilation was refused for a transient reason (code cap, memory limit): count again from the
    /// start and retry later.
    Retry,
    /// BUNDLE-1: leave the prototype's counter alone (a tail-called frame while the compiler's `tailhook` knob is off).
    Skip,
}

/// Entry points of the compiler, registered once per process by `rn_jit::register`.
#[derive(Clone, Copy)]
pub struct VTable {
    /// Function-entry counter fired for the fresh frame `ci` of `proto` (compile if needed).
    pub hot_call: fn(&mut State, Ci, &crate::func::Proto) -> HotCall,
    /// Loop tick fired in the interpreter frame `ci` about to run `pc`; true = `pc` is an entry of compiled code.
    pub hot_loop: fn(&mut State, Ci, *const u32, u32) -> bool,
    /// Run compiled code for `ci` from `ci.savedpc`.
    pub enter: fn(&mut State, Ci) -> JitExit,
    /// Free the per-state data stored in [`JitSlot`].
    pub free_state: fn(*mut ()),
    /// Free the compiled code behind a `Proto.jit` descriptor (the prototype is being collected); returns
    /// the bytes that were charged with `Mem::charge_ext` for it (refunded by the caller).
    pub free_code: fn(*mut u8) -> usize,
}

static VTABLE: std::sync::OnceLock<VTable> = std::sync::OnceLock::new();

/// Register the compiler; the first registration wins. Returns whether this call installed it.
pub fn install(vt: VTable) -> bool {
    VTABLE.set(vt).is_ok()
}

/// The registered compiler, if any.
pub fn vtable() -> Option<&'static VTable> {
    VTABLE.get()
}

static ENTRY_HOT: AtomicI32 = AtomicI32::new(HOT_THRESHOLD);
static TICK_PERIOD_DYN: AtomicI32 = AtomicI32::new(TICK_PERIOD);

/// Set the thresholds (`RUSTYNAIL_JIT_THRESHOLD=entry,loop`): calls before a function is compiled and
/// loop back-edges per sampling tick. Affects protos and threads created afterwards.
pub fn set_thresholds(entry_calls: i32, loop_edges: i32) {
    ENTRY_HOT.store(
        entry_calls.clamp(1, i32::MAX / (2 * HOT_ENTRY_WEIGHT)) * HOT_ENTRY_WEIGHT,
        Ordering::Relaxed,
    );
    TICK_PERIOD_DYN.store(loop_edges.max(1), Ordering::Relaxed);
}

/// Initial value of `Proto.hot` (`entry_calls * HOT_ENTRY_WEIGHT`).
#[inline]
pub fn initial_hot() -> i32 {
    ENTRY_HOT.load(Ordering::Relaxed)
}

/// Back-edge events per sampling tick (counter B).
#[inline]
pub fn tick_period() -> i32 {
    TICK_PERIOD_DYN.load(Ordering::Relaxed)
}

/// Per-universe (global state) storage for the compiler: an opaque pointer, released through
/// [`VTable::free_state`] when the state is closed.
pub struct JitSlot {
    /// Owned by the compiler (created lazily on the first hot event).
    pub data: *mut (),
    /// Status reported by a helper that made compiled code leave with [`JitExit::Error`].
    pub err: Option<Status>,
}

impl JitSlot {
    pub(crate) const fn new() -> JitSlot {
        JitSlot {
            data: std::ptr::null_mut(),
            err: None,
        }
    }
}

/// Records the error status of a helper (read back by the interpreter on [`JitExit::Error`]).
pub fn set_error(st: &mut State, s: Status) {
    st.g_mut().jit.err = Some(s);
}

/// The status recorded by the helper that caused [`JitExit::Error`] (resets it).
pub fn take_error(st: &mut State) -> Status {
    // (`Err(Status::Ok)` is a real value: some natives unwind with it, e.g. closing the running coroutine)
    st.g_mut().jit.err.take().unwrap_or(Status::ErrRun) // None cannot happen: an Error exit records a status
}

/// Called when a prototype is collected: frees its compiled code and refunds the code charge.
///
/// # Safety
/// `p` is a live prototype being freed; no compiled frame of it exists (it is unreachable).
pub(crate) unsafe fn free_proto_code(mem: &mut crate::alloc::Mem, p: &crate::func::Proto) {
    let d = p.jit.get();
    if (d as usize) > 1 {
        if let Some(vt) = VTABLE.get() {
            let charged = (vt.free_code)(d);
            mem.uncharge_ext(charged);
        }
        p.jit.set(std::ptr::null_mut());
    }
}

static BUILTIN_NEXT: AtomicUsize = AtomicUsize::new(0);
static BUILTIN_IPAIRS_AUX: AtomicUsize = AtomicUsize::new(0);

/// Registers the addresses of the base library's `next` and `ipairs` iterator (`ipairs_aux`) so that the
/// generic-`for` helper can run them without a call frame (`jit_helpers::jit_tforcall`). The functions are
/// recognised by address; 0 = unknown (the helper then always calls the iterator).
pub fn set_builtins(next: usize, ipairs_aux: usize) {
    BUILTIN_NEXT.store(next, Ordering::Relaxed);
    BUILTIN_IPAIRS_AUX.store(ipairs_aux, Ordering::Relaxed);
    // T2: with a compiler the interpreter keeps calling the iterators (see `frameless`)
    crate::frameless::disable();
}

/// Address of the registered `next` builtin (0 = none).
#[inline]
pub fn builtin_next() -> usize {
    BUILTIN_NEXT.load(Ordering::Relaxed)
}

/// Address of the registered `ipairs` iterator (0 = none).
#[inline]
pub fn builtin_ipairs_aux() -> usize {
    BUILTIN_IPAIRS_AUX.load(Ordering::Relaxed)
}

/// Number of `math` builtins the tier 1.5 regions can run inline (docs/JIT_TIER15.md section 20).
pub const NUM_INTR: usize = 10;
/// Indices into the table of [`set_intrinsics`]: `sqrt abs floor ceil min max sin cos exp log`.
pub mod intr {
    /// `math.sqrt`
    pub const SQRT: u8 = 0;
    /// `math.abs`
    pub const ABS: u8 = 1;
    /// `math.floor`
    pub const FLOOR: u8 = 2;
    /// `math.ceil`
    pub const CEIL: u8 = 3;
    /// `math.min`
    pub const MIN: u8 = 4;
    /// `math.max`
    pub const MAX: u8 = 5;
    /// `math.sin`
    pub const SIN: u8 = 6;
    /// `math.cos`
    pub const COS: u8 = 7;
    /// `math.exp`
    pub const EXP: u8 = 8;
    /// `math.log`
    pub const LOG: u8 = 9;
}

static BUILTIN_INTR: [AtomicUsize; NUM_INTR] = [const { AtomicUsize::new(0) }; NUM_INTR];

/// Registers the addresses of the `math` functions that regions run inline (indices of [`intr`]; 0 = unknown).
/// Region code recognises the callee of a `CALL` by comparing the light C function payload with the address.
pub fn set_intrinsics(addrs: [usize; NUM_INTR]) {
    for (c, a) in BUILTIN_INTR.iter().zip(addrs) {
        c.store(a, Ordering::Relaxed);
    }
}

/// Address registered for intrinsic `i` (0 = none).
#[inline]
pub fn intrinsic_addr(i: u8) -> usize {
    BUILTIN_INTR
        .get(usize::from(i))
        .map_or(0, |c| c.load(Ordering::Relaxed))
}

/// The intrinsic index whose registered address is `addr` (never for 0).
pub fn intrinsic_of(addr: usize) -> Option<u8> {
    if addr == 0 {
        return None;
    }
    (0..NUM_INTR as u8).find(|&i| intrinsic_addr(i) == addr)
}

/// P-B: number of library functions the J5 recorder knows (indices of [`libfn`]).
pub const NUM_LIBFN: usize = 14;
/// Indices into the table of [`set_libfns`].
pub mod libfn {
    /// `string.sub`
    pub const STR_SUB: u8 = 0;
    /// `string.byte`
    pub const STR_BYTE: u8 = 1;
    /// `string.char`
    pub const STR_CHAR: u8 = 2;
    /// `math.type`
    pub const MATH_TYPE: u8 = 3;
    /// `tostring`
    pub const TOSTRING: u8 = 4;
    /// `rawlen`
    pub const RAWLEN: u8 = 5;
    /// `rawequal`
    pub const RAWEQUAL: u8 = 6;
    /// `rawget`
    pub const RAWGET: u8 = 7;
    /// `rawset`
    pub const RAWSET: u8 = 8;
    /// `table.insert`
    pub const TBL_INSERT: u8 = 9;
    /// `table.remove`
    pub const TBL_REMOVE: u8 = 10;
    /// `math.fmod`
    pub const MATH_FMOD: u8 = 11;
    /// `math.tointeger`
    pub const MATH_TOINTEGER: u8 = 12;
    /// `tonumber`
    pub const TONUMBER: u8 = 13;
    /// `table.unpack` (BUNDLE-R: registered by `set_libfns_ext`)
    pub const TBL_UNPACK: u8 = 14;
    /// `table.move` (BUNDLE-R: registered by `set_libfns_ext`)
    pub const TBL_MOVE: u8 = 15;
}

static BUILTIN_LIBFN: [AtomicUsize; NUM_LIBFN] = [const { AtomicUsize::new(0) }; NUM_LIBFN];
static BUILTIN_TYPEFN: AtomicUsize = AtomicUsize::new(0);

/// Registers the addresses of the library functions the J5 recorder runs as intrinsics (indices of [`libfn`]; 0 = unknown) and the
/// address of `type` (a C *closure* with the type names as upvalues: the helper checks the closure's function pointer at run time).
pub fn set_libfns(addrs: [usize; NUM_LIBFN], type_fn: usize) {
    for (c, a) in BUILTIN_LIBFN.iter().zip(addrs) {
        c.store(a, Ordering::Relaxed);
    }
    BUILTIN_TYPEFN.store(type_fn, Ordering::Relaxed);
}

/// BUNDLE-R: number of library functions registered by [`set_libfns_ext`] (indices `NUM_LIBFN ..` of [`libfn`]).
pub const NUM_LIBFN_EXT: usize = 2;
static BUILTIN_LIBFN_EXT: [AtomicUsize; NUM_LIBFN_EXT] =
    [const { AtomicUsize::new(0) }; NUM_LIBFN_EXT];

/// BUNDLE-R: registers the addresses of `table.unpack` and `table.move` (indices [`libfn::TBL_UNPACK`], [`libfn::TBL_MOVE`]; 0 = unknown).
pub fn set_libfns_ext(addrs: [usize; NUM_LIBFN_EXT]) {
    for (c, a) in BUILTIN_LIBFN_EXT.iter().zip(addrs) {
        c.store(a, Ordering::Relaxed);
    }
}

/// The [`libfn`] index whose registered address is `addr` (never for 0).
pub fn libfn_of(addr: usize) -> Option<u8> {
    if addr == 0 {
        return None;
    }
    (0..NUM_LIBFN as u8)
        .find(|&i| BUILTIN_LIBFN[usize::from(i)].load(Ordering::Relaxed) == addr)
        .or_else(|| {
            (0..NUM_LIBFN_EXT)
                .find(|&i| BUILTIN_LIBFN_EXT[i].load(Ordering::Relaxed) == addr)
                .map(|i| (NUM_LIBFN + i) as u8)
        })
}

static BUILTIN_STRFORMAT: AtomicUsize = AtomicUsize::new(0);

/// BUNDLE-G1: registers the address of `string.format` (0 = unknown).
pub fn set_strformat(addr: usize) {
    BUILTIN_STRFORMAT.store(addr, Ordering::Relaxed);
}

/// BUNDLE-G1: the address of `string.format` (0 = none).
pub fn strformat_addr() -> usize {
    BUILTIN_STRFORMAT.load(Ordering::Relaxed)
}

static BUILTIN_STRLEN: AtomicUsize = AtomicUsize::new(0);

/// F1: registers the address of `string.len` (0 = unknown).
pub fn set_strlen(addr: usize) {
    BUILTIN_STRLEN.store(addr, Ordering::Relaxed);
}

/// F1: the address of `string.len` (0 = none).
pub fn strlen_addr() -> usize {
    BUILTIN_STRLEN.load(Ordering::Relaxed)
}

static BUILTIN_SELECT: AtomicUsize = AtomicUsize::new(0);

/// BUNDLE-T1I: registers the address of `select` (0 = unknown; the tier-1 `CALL` template inlines `select('#', ...)`).
pub fn set_select(addr: usize) {
    BUILTIN_SELECT.store(addr, Ordering::Relaxed);
}

/// BUNDLE-T1I: the address of `select` (0 = none).
pub fn select_addr() -> usize {
    BUILTIN_SELECT.load(Ordering::Relaxed)
}

/// BUNDLE-T1I: the registered address of library function `i` of [`libfn`] (0 = none).
pub fn libfn_addr(i: u8) -> usize {
    BUILTIN_LIBFN
        .get(usize::from(i))
        .map_or(0, |c| c.load(Ordering::Relaxed))
}

/// Address of `type`'s function (0 = none).
pub fn type_fn_addr() -> usize {
    BUILTIN_TYPEFN.load(Ordering::Relaxed)
}

static TYPE_MT_HOOK: AtomicUsize = AtomicUsize::new(0);

/// P-V: registers the compiler's reaction to a replaced string metatable (traces embed it as a constant).
pub fn set_type_mt_hook(f: fn(&mut State)) {
    TYPE_MT_HOOK.store(f as usize, Ordering::Relaxed);
}

/// Called after `g.mt[String]` was assigned (`set_type_metatable`, `set_metatable` of a string): the compiler flushes the
/// traces that resolved `s:method` through the old metatable. A no-op while the compiler has created no data.
pub(crate) fn type_mt_changed(st: &mut State) {
    let f = TYPE_MT_HOOK.load(Ordering::Relaxed);
    if f != 0 && !st.g().jit.data.is_null() {
        // SAFETY: the value was stored by `set_type_mt_hook` from a `fn(&mut State)`.
        let f: fn(&mut State) = unsafe { std::mem::transmute::<usize, fn(&mut State)>(f) };
        f(st);
    }
}

static STRESS_STACKMOVE: AtomicBool = AtomicBool::new(false);

/// `RUSTYNAIL_JIT_STRESS=stackmove`: every helper that can reallocate the stack moves it to a new block on
/// return, so a stale `base` pointer in compiled code reads poisoned memory.
pub fn set_stress_stackmove(on: bool) {
    STRESS_STACKMOVE.store(on, Ordering::Relaxed);
}

/// Whether the stackmove stress is on.
#[inline]
pub fn stress_stackmove() -> bool {
    STRESS_STACKMOVE.load(Ordering::Relaxed)
}

static STRESS_MOVES: AtomicU64 = AtomicU64::new(0);

/// Number of stack moves performed by the stackmove stress so far (tests assert it is non-zero).
pub fn stress_moves() -> u64 {
    STRESS_MOVES.load(Ordering::Relaxed)
}

/// Moves the stack of `st` to a fresh block (stress mode); see `state::move_stack`.
pub fn stress_move_stack(st: &mut State) {
    STRESS_MOVES.fetch_add(1, Ordering::Relaxed);
    let th = st.th_ptr();
    // SAFETY: ST1; `mem` and the thread are disjoint objects.
    unsafe {
        crate::state::move_stack(th, &mut (*(*th).g).heap.mem);
    }
}

impl Drop for JitSlot {
    fn drop(&mut self) {
        // a state being closed ends the recording it armed on this OS thread (a reused address must not match); a
        // recording of another state stays
        #[cfg(feature = "jit_trace")]
        rec_forget_owner(self as *const JitSlot as usize);
        if !self.data.is_null() {
            if let Some(vt) = VTABLE.get() {
                (vt.free_state)(self.data);
            }
            self.data = std::ptr::null_mut();
        }
    }
}

/// The compiler's slot of `st`'s global state.
pub fn slot(st: &mut State) -> &mut JitSlot {
    &mut st.g_mut().jit
}

/// Run compiled code for `ci` (called by the interpreter's `'frame` head when the frame carries
/// [`TRAP_JIT`]).
#[inline(never)]
pub fn enter(st: &mut State, ci: Ci) -> JitExit {
    match VTABLE.get() {
        Some(vt) => (vt.enter)(st, ci),
        None => JitExit::Continue,
    }
}

/// Refund a charge made with [`charge_code`] (failed compile).
pub fn uncharge_code(st: &mut State, bytes: usize) {
    st.g_mut().heap.mem_mut().uncharge_ext(bytes);
}

/// Charge `bytes` of generated code to the state's memory limit (not to `collectgarbage("count")`).
/// False when the limit would be exceeded.
pub fn charge_code(st: &mut State, bytes: usize) -> bool {
    st.g_mut().heap.mem_mut().charge_ext(bytes)
}

/// A `State` handle for the thread pinned in `r12` of generated code, for helpers that live outside this crate
/// (`rn-jit`'s tier 1.5 helpers; the ones in `jit_helpers` use the crate-private constructor directly).
///
/// # Safety
/// `th` must be the live thread that generated code is running on (non-null), and the returned handle must not
/// outlive the helper call nor be moved into storage.
pub unsafe fn state_from_thread(th: *mut u8) -> State {
    // SAFETY: caller contract.
    unsafe { State::from_th_ptr(th.cast()) }
}

/// The thread object a `State` handle designates (machine code addresses its fields through [`thread`]).
pub fn thread_ptr(st: &mut State) -> *mut u8 {
    st.th_ptr().cast()
}

/// N2c: the address of `st`'s `Thread` for a read-only inspection of the fields at [`thread`] offsets.
pub fn thread_ptr_ro(st: &State) -> *const u8 {
    st.th_ptr().cast::<u8>().cast_const()
}

/// Byte offsets inside `Thread`.
pub mod thread {
    use crate::state::Thread;
    use core::mem::offset_of;
    /// `ci`: the running `CallInfo` (pointer).
    pub const CI: usize = offset_of!(Thread, ci);
    /// `top`: first free slot (stack *index*, `usize`).
    pub const TOP: usize = offset_of!(Thread, top);
    /// `openupval`: the highest open upvalue (pointer; null = none). M1: tier 1 tests it to skip `luaF_close`.
    pub const OPENUPVAL: usize = offset_of!(Thread, openupval);
    /// Length word (`usize`) of the thread's list of to-be-closed variables (Lua's `OP_TBC` registers there; it does not set `CIST_TBC`).
    pub const TBC_LEN: usize = offset_of!(Thread, tbc) + crate::func::TbcList::LEN_OFFSET;
    /// `hookmask` (byte): non-zero while a hook is installed.
    pub const HOOKMASK: usize = offset_of!(Thread, hookmask);
    /// `ncalls` (`u32`): low 16 bits = nested C calls (`nCcalls`), high 16 bits = non-yieldable calls.
    pub const NCALLS: usize = offset_of!(Thread, ncalls);
    /// `stack_size` (`usize`): usable slots.
    pub const STACK_SIZE: usize = offset_of!(Thread, stack_size);

    /// Offset (from the `Thread`) of the stack buffer's data pointer. The `RawBuf` field order is a Rust
    /// layout detail, so it is found by searching the buffer's bytes for the pointer of a live thread.
    pub fn stack_ptr_offset(st: &mut crate::state::State) -> usize {
        use core::mem::size_of;
        let base = st.th_ptr().cast::<u8>().cast_const();
        let want = st.slot_ptr(crate::state::StkId(0)) as usize;
        (0..size_of::<crate::alloc::RawBuf<crate::value::Value>>())
            .step_by(size_of::<usize>())
            .map(|d| offset_of!(Thread, stack) + d)
            // SAFETY: inside the `Thread` (the `stack` field); bytes are only inspected.
            .find(|&o| unsafe { base.add(o).cast::<usize>().read_unaligned() } == want)
            .expect("stack pointer not found in Thread.stack")
    }
}

/// Address of the process-wide interrupt flag (a `bool`) that compiled back-edges poll.
pub fn interrupt_flag_addr() -> usize {
    (&raw const crate::state::INTERRUPT) as usize
}

/// P-L: address of the one-byte word a J5 trace polls once per iteration (`state::TRACE_POLL`: set by an interrupt
/// request and by a hook installation, cleared by the trace's poll exit).
pub fn trace_poll_flag_addr() -> usize {
    (&raw const crate::state::TRACE_POLL) as usize
}

// ---- the trace recorder hook (feature `jit_trace`, docs/JIT_TRACE.md 4.1/4.2, TR23) ---------------------------

/// What the recorder tells the interpreter after a step (4.2).
#[cfg(feature = "jit_trace")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecStep {
    /// Keep recording: execute the instruction in the hooked copy.
    Keep,
    /// Recording finished or aborted: return to the fast copy at this pc.
    Leave,
}

/// Why the VM tells the recorder to stop (the VM-side causes; the recorder's own abort reasons are its business).
#[cfg(feature = "jit_trace")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abort {
    /// An instruction raised (the `run()` choke point, TR23.5).
    Error,
    /// A callback from an unexpected frame or thread, or a frame that failed the lazy validation.
    Frame,
    /// A hook was installed.
    Hook,
    /// The state is being closed.
    Close,
}

/// The recorder hook interface (4.2). No fields live in [`JitSlot`]: the recorder's own state is behind
/// `JitSlot.data`; the VM-side context ([`RecCtx`]) is a per-OS-thread cell (cold paths only).
#[cfg(feature = "jit_trace")]
#[derive(Clone, Copy)]
pub struct RecVTable {
    /// Before each instruction of a recorded frame; `*const u32` is the instruction about to execute, the `u64`
    /// the token of the recording (`RecCtx.token`, returned by [`rec_arm`]): a recorder treats a token that is not
    /// the one it armed as a no-op (`Leave` for `step`).
    pub step: fn(&mut State, Ci, *const u32, u64) -> RecStep,
    /// Error unwinding, coroutine switch, close; the token as for `step`. Only called for the state that armed
    /// the recording (`RecCtx.g`).
    pub abort: fn(&mut State, Abort, u64),
}

#[cfg(feature = "jit_trace")]
static REC_VT: std::sync::OnceLock<RecVTable> = std::sync::OnceLock::new();

/// Register the recorder; the first registration wins. Returns whether this call installed it.
#[cfg(feature = "jit_trace")]
pub fn install_rec(vt: RecVTable) -> bool {
    REC_VT.set(vt).is_ok()
}

/// The VM's view of the armed recording of this OS thread (all fields are addresses, never dereferenced without
/// the validation of [`rec_step`]); `th == 0` means "not recording". At most one recording per OS thread (TR16).
#[cfg(feature = "jit_trace")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecCtx {
    /// The recording thread (`Thread` address).
    pub th: usize,
    /// The root `CallInfo` when recording was armed.
    pub root: usize,
    /// Its function slot index at that time.
    pub root_func: usize,
    /// The root closure (object address) at that time.
    pub root_cl: usize,
    /// Innermost frame stepped so far (the root until the first step).
    pub cur: usize,
    /// The recording installed a trace: on `Leave` flag the frame with [`TRAP_JIT`].
    pub enter: bool,
    /// The owning state: the address of its `JitSlot` (one per global state; never dereferenced). Another state
    /// on the same OS thread must not touch the context.
    pub g: usize,
    /// Monotonic id of the recording (global counter taken at [`rec_arm`]); never 0 for an armed recording.
    pub token: u64,
}

/// The per-OS-thread cell. Its `Drop` (thread exit) gives back the `REC_ARMED` count of a context still armed.
#[cfg(feature = "jit_trace")]
struct RecCell(std::cell::Cell<RecCtx>);

#[cfg(feature = "jit_trace")]
impl Drop for RecCell {
    fn drop(&mut self) {
        if self.0.get().th != 0 {
            REC_ARMED.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(feature = "jit_trace")]
static REC_TOKEN: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "jit_trace")]
thread_local! {
    static REC: RecCell = const { RecCell(std::cell::Cell::new(RecCtx { th: 0, root: 0, root_func: 0, root_cl: 0, cur: 0, enter: false, g: 0, token: 0 })) };
}

/// Maximum number of frames between the current frame and the root that the lazy validation walks.
#[cfg(feature = "jit_trace")]
const REC_MAX_WALK: usize = 64;

/// Is a recording armed on this OS thread?
#[cfg(feature = "jit_trace")]
#[inline]
pub fn rec_active() -> bool {
    REC.with(|c| c.0.get().th != 0)
}

/// Is a recording armed for the thread of `st`?
#[cfg(feature = "jit_trace")]
#[inline]
pub fn rec_active_for(st: &mut State) -> bool {
    let g = rec_owner(st);
    REC.with(|c| {
        let c = c.0.get();
        c.th == st.th_ptr() as usize && c.g == g
    })
}

/// The identity of `st`'s global state for [`RecCtx::g`]: the address of its `JitSlot`.
#[cfg(feature = "jit_trace")]
#[inline]
fn rec_owner(st: &mut State) -> usize {
    (&raw const st.g().jit) as usize
}

/// Test support: the context of this OS thread.
#[cfg(feature = "jit_trace")]
#[doc(hidden)]
pub fn rec_ctx() -> RecCtx {
    REC.with(|c| c.0.get())
}

/// Number of armed recordings (all OS threads): `fast_trap` reads it with a plain load (see there).
#[cfg(feature = "jit_trace")]
static REC_ARMED: AtomicUsize = AtomicUsize::new(0);

/// Is any recording armed (any OS thread)? A single relaxed load: the fast copy's cold `fast_trap` uses it.
#[cfg(feature = "jit_trace")]
#[inline(always)]
pub fn rec_armed() -> bool {
    REC_ARMED.load(Ordering::Relaxed) != 0
}

/// Test support: the owner identity ([`RecCtx::g`]) of `st`.
#[cfg(feature = "jit_trace")]
#[doc(hidden)]
pub fn rec_owner_id(st: &mut State) -> usize {
    rec_owner(st)
}

/// `JitSlot::drop`: forget the context of this OS thread if it belongs to the slot at `owner` (`try_with`: the
/// thread-local may already be gone during thread teardown, its `Drop` then did the count).
#[cfg(feature = "jit_trace")]
fn rec_forget_owner(owner: usize) {
    let _ = REC.try_with(|r| {
        let c = r.0.get();
        if c.th != 0 && c.g == owner {
            r.0.set(RecCtx::default());
            REC_ARMED.fetch_sub(1, Ordering::Relaxed);
        }
    });
}

/// `State::close_state`: a recording armed by the closing state ends with `Abort::Close` (delivered to the
/// recorder); a recording of another state is left alone.
#[cfg(feature = "jit_trace")]
pub fn rec_close(st: &mut State) {
    let c = rec_ctx();
    if c.th != 0 && c.g == rec_owner(st) {
        rec_abort_any(st, Abort::Close);
    }
}

/// Test support: overwrite the context of this OS thread (the ABA test restores a stale one).
#[cfg(feature = "jit_trace")]
#[doc(hidden)]
pub fn rec_set_ctx(c: RecCtx) {
    let old = REC.with(|r| r.0.replace(c));
    match (old.th != 0, c.th != 0) {
        (false, true) => {
            REC_ARMED.fetch_add(1, Ordering::Relaxed);
        }
        (true, false) => {
            REC_ARMED.fetch_sub(1, Ordering::Relaxed);
        }
        _ => {}
    }
}

/// Arm a recording rooted at the Lua frame `ci` of `st` (`jit_tr_hot`): the frame gets `trap = 1`, so the fast copy
/// hands it to the hooked copy at its next trap poll (`fast_trap`). Returns the token of the recording (passed to
/// the recorder's callbacks), or `None` (nothing armed) when `ci` is not a Lua frame, a hook is installed or a
/// recording is already armed on this OS thread.
///
/// Arm contract: the arming site hands the frame off immediately: it returns to the interpreter without running
/// anything that can raise before the frame's next trap poll. If an error nevertheless unwinds first, `run()`'s
/// error choke point (fast copy included) aborts the recording.
#[cfg(feature = "jit_trace")]
pub fn rec_arm(st: &mut State, ci: Ci) -> Option<u64> {
    if st.hook_mask() != 0 || rec_active() {
        return None;
    }
    let cl = st.ci_lclosure(ci)?;
    let token = REC_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;
    rec_set_ctx(RecCtx {
        th: st.th_ptr() as usize,
        root: ci.as_ptr() as usize,
        root_func: ci.r().func.0,
        root_cl: cl.as_ptr() as usize,
        cur: ci.as_ptr() as usize,
        enter: false,
        g: rec_owner(st),
        token,
    });
    ci.w().trap = 1;
    Some(token)
}

/// S1: [`rec_arm`] for a recording whose first step is in `inner`, a Lua frame above the root `root` (a side recording that
/// starts inside inlined frames that the exit restored): `trap` is set on every Lua frame from `inner` down to `root`.
#[cfg(feature = "jit_trace")]
pub fn rec_arm_chain(st: &mut State, root: Ci, inner: Ci) -> Option<u64> {
    if st.hook_mask() != 0 || rec_active() {
        return None;
    }
    let cl = st.ci_lclosure(root)?;
    let token = REC_TOKEN.fetch_add(1, Ordering::Relaxed) + 1;
    rec_set_ctx(RecCtx {
        th: st.th_ptr() as usize,
        root: root.as_ptr() as usize,
        root_func: root.r().func.0,
        root_cl: cl.as_ptr() as usize,
        cur: inner.as_ptr() as usize,
        enter: false,
        g: rec_owner(st),
        token,
    });
    let mut cur = Some(inner);
    for _ in 0..REC_MAX_WALK {
        let Some(c) = cur else { break };
        if c.r().is_lua() {
            c.w().trap = 1;
        }
        if c.as_ptr() as usize == root.as_ptr() as usize {
            break;
        }
        cur = c.previous();
    }
    Some(token)
}

/// N2a (RETF): the recorder is about to record the `RETURN` of the root frame and continue in the Lua frame `caller` it returns
/// into (a native pop, docs/JIT_PARKED.md C38). Rebinds the recording's root to `caller` (`root`, `root_func`, `root_cl`), makes it
/// the frame the recorder expects next (`cur`) and sets its `trap`, so that the interpreter, which performs the `RETURN` in the same
/// hooked run (`do_ret!`: `ci = previous; continue 'frame`), hands the first instruction after the call to the recorder through
/// [`rec_step`], whose validation then reaches the new root. Returns `false` (nothing changed) when there is no armed recording of
/// `st`, a hook is installed, or `caller` is not a Lua frame.
#[cfg(feature = "jit_trace")]
pub fn rec_reroot(st: &mut State, caller: Ci) -> bool {
    let c = rec_ctx();
    if c.th == 0 || c.th != st.th_ptr() as usize || c.g != rec_owner(st) || st.hook_mask() != 0 {
        return false;
    }
    let Some(cl) = st.ci_lclosure(caller) else {
        return false;
    };
    let mut v = c;
    v.root = caller.as_ptr() as usize;
    v.root_func = caller.r().func.0;
    v.root_cl = cl.as_ptr() as usize;
    v.cur = caller.as_ptr() as usize;
    REC.with(|r| r.0.set(v));
    caller.w().trap = 1;
    true
}

/// S1: forget an armed recording whose recorder was never installed (the recorder refused its start): clears the context and the
/// traps without calling the recorder.
#[cfg(feature = "jit_trace")]
pub fn rec_disarm(st: &mut State) {
    let c = rec_ctx();
    if c.th == 0 || c.g != rec_owner(st) {
        return;
    }
    rec_set_ctx(RecCtx::default());
    rec_clear_traps(st, c.root);
}

/// The recording installed a trace: the next `Leave` flags the frame with [`TRAP_JIT`].
#[cfg(feature = "jit_trace")]
pub fn rec_request_enter() {
    REC.with(|c| {
        let mut v = c.0.get();
        v.enter = true;
        c.0.set(v);
    });
}

/// `fast_trap` / hooked `'frame` head (TR23.1/2): does the recorder expect `ci` to run next? True for the frame it
/// stepped last (resumption) and for a frame just pushed above it (an inlined callee).
#[cfg(feature = "jit_trace")]
pub fn rec_wants(st: &mut State, ci: Ci) -> bool {
    let c = rec_ctx();
    c.th != 0
        && c.th == st.th_ptr() as usize
        && c.g == rec_owner(st)
        && st.hook_mask() == 0
        && (ci.as_ptr() as usize == c.cur
            || ci.r().previous as usize == c.cur
            || rec_via_native(ci, c.cur))
}

/// P-O: is `ci` a Lua frame called by a C function (`pcall` / `xpcall`) that the recording's last frame `cur` called?
#[cfg(feature = "jit_trace")]
fn rec_via_native(ci: Ci, cur: usize) -> bool {
    let prev = ci.r().previous;
    if prev.is_null() {
        return false;
    }
    // SAFETY: `previous` of a live `CallInfo` is a live node of the same list (or null, checked above).
    let p = unsafe { &*prev };
    p.callstatus & crate::state::CIST_C != 0 && p.previous as usize == cur
}

/// Test-only VM mutant `hooked_head_no_trap` (docs/JIT_TRACE.md 9.7): the hooked `'frame` head does not set `trap`.
#[cfg(feature = "jit_trace_mut")]
static MUT_HOOKED_HEAD_NO_TRAP: AtomicBool = AtomicBool::new(false);

/// Select the mutant `hooked_head_no_trap` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_hooked_head_no_trap(on: bool) {
    MUT_HOOKED_HEAD_NO_TRAP.store(on, Ordering::Relaxed);
}

/// Test-only VM mutant `leave_trap_kept` (docs/JIT_TRACE.md 9.7): `rec_clear_traps` leaves the callers' `trap` set.
#[cfg(feature = "jit_trace_mut")]
static MUT_LEAVE_TRAP_KEPT: AtomicBool = AtomicBool::new(false);

/// Select the mutant `leave_trap_kept` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_leave_trap_kept(on: bool) {
    MUT_LEAVE_TRAP_KEPT.store(on, Ordering::Relaxed);
}

/// The hooked copy's `'frame` head under recording (TR23.2): keeps the frame in the hooked copy.
#[cfg(feature = "jit_trace")]
#[cold]
#[inline(never)]
pub fn rec_head(st: &mut State, ci: Ci) {
    #[cfg(feature = "jit_trace_mut")]
    if MUT_HOOKED_HEAD_NO_TRAP.load(Ordering::Relaxed) {
        return;
    }
    if rec_wants(st, ci) {
        ci.w().trap = 1;
    }
}

/// Lazy validation (TR23.5): does `ci` (about to execute `pc`) belong to the armed recording? The thread is the
/// recording thread, the chain of `ci` leads to the root `CallInfo` through Lua frames only (bounded), every frame
/// holds a Lua closure with `savedpc` (`pc` for `ci`) inside its prototype, and the root still has the function
/// slot index and closure it had when armed (a reused `CallInfo` node after an unwind fails here).
#[cfg(feature = "jit_trace")]
fn rec_validate(st: &mut State, c: &RecCtx, ci: Ci, pc: *const u32) -> bool {
    if c.th != st.th_ptr() as usize || c.g != rec_owner(st) {
        return false;
    }
    let mut cur = ci;
    for _ in 0..REC_MAX_WALK {
        let Some(cl) = st.ci_lclosure(cur) else {
            // P-O: the native frame of an inlined `pcall` between two Lua frames
            if cur.as_ptr() != ci.as_ptr() && cur.r().callstatus & crate::state::CIST_C != 0 {
                match cur.previous() {
                    Some(prev) => {
                        cur = prev;
                        continue;
                    }
                    None => return false,
                }
            }
            return false;
        };
        let p = cl.proto();
        let code = p.code_ptr();
        // SAFETY: one-past-the-end pointer of the code array of a live prototype.
        let end = unsafe { code.add(p.code().len()) };
        let at = if cur.as_ptr() == ci.as_ptr() {
            pc
        } else {
            cur.r().savedpc
        };
        if at < code || at > end {
            return false;
        }
        if cur.as_ptr() as usize == c.root {
            return cur.r().func.0 == c.root_func && cl.as_ptr() as usize == c.root_cl;
        }
        match cur.previous() {
            Some(prev) => cur = prev,
            None => return false,
        }
    }
    false
}

/// Clear `trap` on the Lua frames from `st`'s current frame up to the root (TR23.4; nothing when a hook is
/// installed, the hooks need their `trap`).
#[cfg(feature = "jit_trace")]
fn rec_clear_traps(st: &mut State, root: usize) {
    #[cfg(feature = "jit_trace_mut")]
    if MUT_LEAVE_TRAP_KEPT.load(Ordering::Relaxed) {
        return;
    }
    if st.hook_mask() != 0 {
        return;
    }
    let mut cur = Some(st.ci());
    for _ in 0..REC_MAX_WALK {
        let Some(c) = cur else { break };
        if c.r().is_lua() && c.r().trap == 1 {
            c.w().trap = 0;
        }
        if c.as_ptr() as usize == root {
            break;
        }
        cur = c.previous();
    }
}

/// Before each instruction of a frame of the hooked copy while a recording is armed (TR23.3/4): validates, calls
/// the recorder and, on `Leave`, prepares the frame for the fast copy (`savedpc = pc`, `trap`, callers' `trap`
/// cleared). `Leave` also ends the recording on every validation failure.
#[cfg(feature = "jit_trace")]
#[cold]
#[inline(never)]
pub fn rec_step(st: &mut State, ci: Ci, pc: *const u32) -> RecStep {
    let c = rec_ctx();
    if c.th == 0 {
        return RecStep::Keep;
    }
    if c.g != rec_owner(st) {
        // a frame of another state on this OS thread (it entered the hooked copy through `fast_trap`'s
        // over-approximation): not ours, hand it back and leave the context (and the recorder) alone
        ci.w().savedpc = pc;
        ci.w().trap = i32::from(st.hook_mask() != 0);
        return RecStep::Leave;
    }
    let why = if c.th != st.th_ptr() as usize || !rec_validate(st, &c, ci, pc) {
        Some(Abort::Frame)
    } else if st.hook_mask() != 0 {
        Some(Abort::Hook)
    } else {
        None
    };
    let step = match (why, REC_VT.get()) {
        (None, Some(vt)) => {
            REC.with(|r| {
                let mut v = r.0.get();
                v.cur = ci.as_ptr() as usize;
                r.0.set(v);
            });
            (vt.step)(st, ci, pc, c.token)
        }
        (Some(a), _) => {
            rec_abort_any(st, a);
            RecStep::Leave
        }
        (None, None) => {
            rec_abort_any(st, Abort::Close);
            RecStep::Leave
        }
    };
    if step == RecStep::Leave {
        // the recorder finished (or aborted itself, or the VM aborted it): end the recording and hand the
        // frame to the fast copy
        let enter = rec_ctx().enter;
        rec_set_ctx(RecCtx::default());
        rec_clear_traps(st, c.root);
        ci.w().savedpc = pc;
        ci.w().trap = if st.hook_mask() != 0 {
            1
        } else if enter {
            TRAP_JIT
        } else {
            0
        };
    }
    step
}

/// Abort the recording of `st`'s thread, if any (`run()` choke point after an `Err`, hook installed, close): calls
/// the recorder's `abort`, forgets the context and clears `trap` on the frames that still exist.
#[cfg(feature = "jit_trace")]
#[cold]
#[inline(never)]
pub fn rec_abort(st: &mut State, why: Abort) {
    if rec_active_for(st) {
        rec_abort_any(st, why);
    }
}

/// [`rec_abort`] for the armed recording of this OS thread whichever thread of the owning state `st` is (the
/// validation of `rec_step` found a frame of another thread, or the state is closing). Does nothing when the
/// recording belongs to another state.
#[cfg(feature = "jit_trace")]
fn rec_abort_any(st: &mut State, why: Abort) {
    let c = rec_ctx();
    if c.th == 0 || c.g != rec_owner(st) {
        return;
    }
    rec_set_ctx(RecCtx::default());
    if let Some(vt) = REC_VT.get() {
        (vt.abort)(st, why, c.token);
    }
    rec_clear_traps(st, c.root);
}
