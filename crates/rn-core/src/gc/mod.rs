//! G1 garbage collector: stop-the-world, precise, non-moving mark & sweep (port of the relevant parts
//! of `lgc.c`, Lua 5.5). ADR-4.
//!
//! The object model follows the reference so the incremental (G2) and generational (G3) collectors can
//! be layered on later: tri-colour marks with *current/other white*, gray lists (`gray`, `grayagain`,
//! `weak`, `allweak`, `ephemeron`), `finobj`/`tobefnz` finalizer lists, `fixedgc` for never-collected
//! objects. A collection runs start-to-finish inside [`Heap::collect`]; the state machine phases are
//! kept so the same functions can be driven step by step later.
//!
//! Collection happens only at explicit safepoints: [`Heap::check_gc`] (threshold based, see
//! [`GcParams`]) or [`Heap::collect`]. **GC stress:** if the environment variable
//! `RUSTYNAIL_GC_STRESS=1` is set when a `Heap` is created (or [`Heap::set_gc_stress`] is called), every
//! `check_gc` safepoint performs a full collection (PLAN V8).
//!
//! Roots: the heap does not know the interpreter state, so callers pass a *root-marking callback*
//! (`FnMut(&mut Marker)`) to `collect`/`check_gc`; it is invoked at the start of the cycle and again at
//! the atomic phase (as the reference re-marks the registry and global metatables there).
//!
//! Closures, upvalues, protos, userdata and threads (task B3) are traversed/freed in `obj.rs`; the
//! per-kind dispatch tables below call into it. Finalizer *calls* are made by the caller of
//! [`Heap::collect_no_finalizers`] (see `collect.rs`), never from inside the collector, so the heap is
//! not borrowed while Lua code runs.
//!
//! `unsafe` here is raw traversal of the object graph. **Invariants:** (G1) every pointer reachable
//! from the lists/roots is a live object of this heap; (G2) outside `collect`, no object is gray or
//! carries the other white (all are current white — "black" objects exist only transiently inside a
//! cycle); (G3) objects on `finobj`/`tobefnz` have `FINALIZEDBIT` set, and are not on `allgc`.
use crate::func::{size_cclosure, size_lclosure, CClosure, LClosure, Proto, UpVal};
use crate::heap::Heap;
use crate::meta::Tm;
use crate::state::{thread_size, Thread};
use crate::string::{counted_size, LuaString, StrRef};
use crate::table::{gcaccess as ga, Table, TableRef};
use crate::udata::{size_udata, Udata};
use crate::value::*;

#[cfg(rustynail_lua54)]
mod gen54;
mod generational;
mod obj;
#[cfg(rustynail_lua54)]
pub use gen54::Host54;
#[cfg(rustynail_lua54)]
pub(crate) use gen54::{
    changemode54 as gen54_changemode, fullgc54 as gen54_fullgc, luac_step54 as gen54_step,
};

pub const WHITE0BIT: u8 = 1 << 3;
pub const WHITE1BIT: u8 = 1 << 4;
pub const BLACKBIT: u8 = 1 << 5;
/// Object has been marked for finalization (`FINALIZEDBIT`).
pub const FINALIZEDBIT: u8 = 1 << 6;
pub const WHITEBITS: u8 = WHITE0BIT | WHITE1BIT;
const MASKCOLORS: u8 = BLACKBIT | WHITEBITS;

// Object ages (generational mode), kept in the low three bits of `marked` (`lgc.h`).
/// Created in the current cycle.
pub const G_NEW: u8 = 0;
/// Created in the previous cycle.
pub const G_SURVIVAL: u8 = 1;
/// Marked old by a forward barrier in this cycle.
pub const G_OLD0: u8 = 2;
/// First full cycle as old.
pub const G_OLD1: u8 = 3;
/// Really old (not visited by minor collections).
pub const G_OLD: u8 = 4;
/// Old object touched (back barrier) in this cycle.
pub const G_TOUCHED1: u8 = 5;
/// Old object touched in the previous cycle.
pub const G_TOUCHED2: u8 = 6;
pub const AGEBITS: u8 = 7;
/// `maskgcbits`: colour and age bits.
const MASKGCBITS: u8 = MASKCOLORS | AGEBITS;

#[inline]
pub(crate) unsafe fn getage(o: *const GcHeader) -> u8 {
    // SAFETY: G1.
    unsafe { (*o).marked & AGEBITS }
}
#[inline]
unsafe fn setage(o: *mut GcHeader, a: u8) {
    // SAFETY: G1.
    unsafe { (*o).marked = ((*o).marked & !AGEBITS) | a }
}
#[inline]
pub(crate) unsafe fn isold(o: *const GcHeader) -> bool {
    // SAFETY: G1.
    unsafe { getage(o) > G_SURVIVAL }
}

/// `GCSTPUSR`, `GCSTPGC`, `GCSTPCLS`.
pub const GCSTP_USER: u8 = 1;
pub const GCSTP_GC: u8 = 2;
pub const GCSTP_CLOSING: u8 = 4;

/// `CommonHeader`: every collectable object starts with this (`#[repr(C)]`, first field).
#[repr(C)]
#[derive(Debug)]
pub struct GcHeader {
    /// Next object in the list the object is on (`allgc`, `finobj`, ...).
    pub next: *mut GcHeader,
    /// Object variant tag (`LUA_V*`, *without* the collectable bit).
    pub tt: u8,
    pub marked: u8,
    /// Spare bytes of the header word, used by strings (`extra`, `shrlen`, `hash`) so that a
    /// string costs no more than in the reference; unused (zero) for other objects.
    pub(crate) extra: std::cell::Cell<u8>,
    pub(crate) shrlen: i8,
    pub(crate) hash: std::cell::Cell<u32>,
}

impl GcHeader {
    /// Header for an object that does not use the spare bytes.
    #[inline]
    pub const fn new(next: *mut GcHeader, tt: u8, marked: u8) -> GcHeader {
        GcHeader {
            next,
            tt,
            marked,
            extra: std::cell::Cell::new(0),
            shrlen: 0,
            hash: std::cell::Cell::new(0),
        }
    }
}

/// Collector phases (`GCS*`). `Atomic` doubles as `GCSenteratomic` between steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GcPhase {
    Pause,
    Propagate,
    Atomic,
    SweepAllGc,
    SweepFinObj,
    SweepToBeFnz,
    SweepEnd,
    CallFin,
}

/// `luaO_codeparam`: encode a percentage as a floating-point byte.
pub fn code_param(p: u32) -> u8 {
    if p as u64 >= (0x1Fu64 << (0xF - 7 - 1)) * 100 {
        return 0xFF;
    }
    let p = (p as u64 * 128).div_ceil(100) as u32;
    if p < 0x10 {
        p as u8
    } else {
        // ceillog2(p + 1) - 5
        let log = (32 - p.leading_zeros()) - 5;
        (((p >> log) - 0x10) | ((log + 1) << 4)) as u8
    }
}

/// `luaO_applyparam`: multiply `x` by the encoded parameter.
pub fn apply_param(p: u8, x: i64) -> i64 {
    let mut m = (p & 0xF) as i64;
    let mut e = (p >> 4) as i32;
    if e > 0 {
        e -= 1;
        m += 0x10;
    }
    e -= 7;
    const MAX: i64 = i64::MAX;
    if e >= 0 {
        if x < (MAX / 0x1F) >> e {
            (x * m) << e
        } else {
            MAX
        }
    } else {
        let e = -e;
        if x < MAX / 0x1F {
            (x * m) >> e
        } else if (x >> e) < MAX / 0x1F {
            (x >> e) * m
        } else {
            MAX
        }
    }
}

/// Indices of the collector parameters (`LUA_GCP*`, in `collectgarbage("param", name)` order).
pub const GCP_MINORMUL: usize = 0;
pub const GCP_MAJORMINOR: usize = 1;
pub const GCP_MINORMAJOR: usize = 2;
pub const GCP_PAUSE: usize = 3;
pub const GCP_STEPMUL: usize = 4;
pub const GCP_STEPSIZE: usize = 5;
pub const GCP_N: usize = 6;

/// Collector parameters (5.5 semantics: stored as encoded percentages, see `lgc.h`).
#[derive(Clone, Copy, Debug)]
pub struct GcParams {
    p: [u8; GCP_N],
    /// BUNDLE-A item 1: has the program set `minormul` explicitly? Until it has, the nursery of a young collection is
    /// a multiple of what `minormul` says (the reported parameter stays the reference's 20).
    pub minormul_user: bool,
    /// The Lua 5.4 parameters as `lua_gc` reports them (see [`Params54`]).
    #[cfg(rustynail_lua54)]
    pub v54: Params54,
}

/// Lua 5.4 collector parameters (`lua_gc` `LUA_GCSETPAUSE`/`LUA_GCSETSTEPMUL`/`LUA_GCINC`/
/// `LUA_GCGEN`), stored like `lgc.h` does (`setgcparam` keeps `v / 4` in a byte, `getgcparam`
/// returns `* 4`). The 5.4 control layer (`gen54.rs`) reads these values directly; setting one
/// also sets the corresponding [`GcParams`] value (pause and minor multiplier as given, step
/// multiplier scaled to the 5.5 default 200%, step size `2^stepsize` bytes), which only the
/// shared helpers use (`steps_for_alloc`).
#[cfg(rustynail_lua54)]
#[derive(Clone, Copy, Debug)]
pub struct Params54 {
    pub pause: u8,
    pub stepmul: u8,
    pub stepsize: u8,
    pub genminormul: u8,
    pub genmajormul: u8,
}

#[cfg(rustynail_lua54)]
impl GcParams {
    /// `getgcparam`.
    pub fn get54(v: u8) -> i64 {
        v as i64 * 4
    }
    /// `setgcparam`.
    pub fn code54(v: i64) -> u8 {
        (v / 4) as u8
    }
    /// `collectgarbage("setpause", v)`: returns the previous value.
    pub fn set_pause54(&mut self, v: i64) -> i64 {
        let old = Self::get54(self.v54.pause);
        self.v54.pause = Self::code54(v);
        self.set(GCP_PAUSE, Self::get54(self.v54.pause) as u32);
        old
    }
    /// `collectgarbage("setstepmul", v)`: returns the previous value.
    pub fn set_stepmul54(&mut self, v: i64) -> i64 {
        let old = Self::get54(self.v54.stepmul);
        self.v54.stepmul = Self::code54(v);
        self.set(GCP_STEPMUL, (Self::get54(self.v54.stepmul) * 2) as u32);
        old
    }
    /// `g->gcstepsize = v` (log2 of the step size in bytes).
    pub fn set_stepsize54(&mut self, v: i64) {
        self.v54.stepsize = v as u8;
        let bytes = 1u64 << (self.v54.stepsize as u32).min(40);
        self.set(GCP_STEPSIZE, bytes.min(u32::MAX as u64) as u32);
    }
    /// `g->genminormul = v`.
    pub fn set_genminormul54(&mut self, v: i64) {
        self.v54.genminormul = v as u8;
        self.set(GCP_MINORMUL, self.v54.genminormul as u32);
    }
    /// `setgcparam(g->genmajormul, v)`.
    pub fn set_genmajormul54(&mut self, v: i64) {
        self.v54.genmajormul = Self::code54(v);
    }
}

impl GcParams {
    /// `LUAI_GENMINORMUL` = 20, `LUAI_MAJORMINOR` = 50, `LUAI_MINORMAJOR` = 70, `LUAI_GCPAUSE` = 250
    /// (200 in 5.4),
    /// `LUAI_GCMUL` = 200, `LUAI_GCSTEPSIZE` = 200 * sizeof(Table).
    pub fn defaults() -> Self {
        let mut p = [0u8; GCP_N];
        p[GCP_MINORMUL] = code_param(20);
        p[GCP_MAJORMINOR] = code_param(50);
        p[GCP_MINORMAJOR] = code_param(70);
        p[GCP_PAUSE] = code_param(if cfg!(rustynail_lua54) { 200 } else { 250 });
        p[GCP_STEPMUL] = code_param(200);
        // the reference value (200 * sizeof(Table) = 9600): user-visible through `collectgarbage("param")`
        p[GCP_STEPSIZE] = code_param(if cfg!(rustynail_lua54) { 1 << 13 } else { 9600 });
        GcParams {
            p,
            minormul_user: false,
            // LUAI_GCPAUSE 200, LUAI_GCMUL 100, LUAI_GCSTEPSIZE 13, LUAI_GENMINORMUL 20,
            // LUAI_GENMAJORMUL 100 (5.4 lgc.h)
            #[cfg(rustynail_lua54)]
            v54: Params54 {
                pause: 200 / 4,
                stepmul: 100 / 4,
                stepsize: 13,
                genminormul: 20,
                genmajormul: 100 / 4,
            },
        }
    }
    /// `lua_gc(LUA_GCPARAM, idx, -1)`: the value as given (a percentage / byte count).
    pub fn get(&self, idx: usize) -> i64 {
        apply_param(self.p[idx], 100)
    }
    /// `lua_gc(LUA_GCPARAM, idx, v)`.
    pub fn set(&mut self, idx: usize, v: u32) {
        if idx == GCP_MINORMUL {
            self.minormul_user = true;
        }
        self.p[idx] = code_param(v);
    }
    pub fn set_pause(&mut self, percent: u32) {
        self.set(GCP_PAUSE, percent);
    }
    pub fn set_stepmul(&mut self, percent: u32) {
        self.set(GCP_STEPMUL, percent);
    }
    pub fn set_stepsize(&mut self, v: u32) {
        self.set(GCP_STEPSIZE, v);
    }
    /// `applygcparam(idx, x)`.
    pub fn apply(&self, idx: usize, x: usize) -> usize {
        apply_param(self.p[idx], x as i64) as usize
    }
    /// `applygcparam(PAUSE, x)`.
    pub fn pause(&self, x: usize) -> usize {
        self.apply(GCP_PAUSE, x)
    }
    /// `applygcparam(STEPMUL, x)`.
    pub fn stepmul(&self, x: usize) -> usize {
        self.apply(GCP_STEPMUL, x)
    }
    /// `applygcparam(STEPSIZE, x)`.
    pub fn stepsize(&self, x: usize) -> usize {
        self.apply(GCP_STEPSIZE, x)
    }
}

/// Collector mode (`g->gckind`). `GenMajor` is "generational mode doing a major (incremental)
/// collection"; `lua_gc` reports it as generational.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcKind {
    Incremental,
    GenMinor,
    GenMajor,
}

impl GcKind {
    pub fn is_generational(self) -> bool {
        self != GcKind::Incremental
    }
}

/// Result of a [`Heap::inc_step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    /// The step finished (or the cycle ended).
    Done,
    /// The collector is in the call-finalizers phase: the caller must run a finalizer.
    NeedFin,
}

/// Work units charged for running one finalizer (`CWUFIN`).
pub const CWUFIN: isize = 10;

#[cfg_attr(rustynail_lua54, allow(dead_code))] // (5.4 never stops after the atomic step)
enum Step {
    Work(usize),
    Atomic,
    /// The collector returned to minor collections.
    Minor,
    NeedFin,
    Paused,
}

/// Hook called for each object whose `__gc` must run (`GCTM`). The object has been returned to the
/// normal object list and its finalized bit cleared; the hook looks up and calls the metamethod.
pub type FinalizerHook = Box<dyn FnMut(&mut Heap, *mut GcHeader)>;

/// Collector state, a field of [`Heap`].
pub struct GcState {
    pub(crate) phase: GcPhase,
    gray: *mut GcHeader,
    grayagain: *mut GcHeader,
    weak: *mut GcHeader,
    allweak: *mut GcHeader,
    ephemeron: *mut GcHeader,
    /// Objects with finalizers (not yet unreachable).
    pub(crate) finobj: *mut GcHeader,
    /// Objects whose finalizer is pending.
    pub(crate) tobefnz: *mut GcHeader,
    /// Objects that are never collected (metamethod names, ...).
    pub(crate) fixedgc: *mut GcHeader,
    /// Bytes marked in the last/current cycle (`GCmarked`).
    pub(crate) marked: usize,
    /// Start a collection at the next safepoint when `Mem::total_bytes()` exceeds this.
    pub(crate) threshold: usize,
    /// P-B: `threshold - total` right after the last step (0 before the first), see [`Heap::jit_gc_step_hint`].
    pub(crate) last_gap: usize,
    /// J5 sinking (TR29.4): the virtual bytes outstanding when the current incremental cycle started; credited back when
    /// the cycle completes.
    #[cfg(feature = "jit_trace")]
    pub(crate) virt_mark: usize,
    /// J5 (F8): objects the trace recorder holds raw handles to (see [`Heap::watch_object`]); freeing one sets `watch_hit`.
    #[cfg(feature = "jit_trace")]
    pub(crate) watch: Vec<usize>,
    #[cfg(feature = "jit_trace")]
    pub(crate) watch_hit: bool,
    pub params: GcParams,
    /// `GCSTP*` bits.
    pub(crate) stp: u8,
    pub(crate) emergency: bool,
    pub(crate) stress: bool,
    /// Stress mode 2: every safepoint performs a one-unit incremental step.
    pub(crate) stress_steps: bool,
    /// `RUSTYNAIL_GC_VERIFY=1`: run [`Heap::verify`] after every incremental step (slow).
    pub(crate) verify_steps: bool,
    pub(crate) in_collect: bool,
    pub(crate) kind: GcKind,
    pub(crate) fin_hook: Option<FinalizerHook>,
    /// Threads with open upvalues (`g->twups`).
    pub(crate) twups: *mut Thread,
    /// Number of completed cycles (diagnostics/tests).
    pub collections: u64,
    /// Incremental sweep cursor: the last surviving object swept in the current list
    /// (null = the list head). Only meaningful in the `Sweep*` phases.
    sweep_prev: *mut GcHeader,
    // Generational mode: boundaries inside `allgc` (order: new ... survival ... old1 ... reallyold)
    // and `finobj` (`finobjsur`, `finobjold1`, `finobjrold`). Null = end of list.
    survival: *mut GcHeader,
    old1: *mut GcHeader,
    reallyold: *mut GcHeader,
    /// First OLD1 object in `allgc` (optimisation for `markold`).
    firstold1: *mut GcHeader,
    finobjsur: *mut GcHeader,
    finobjold1: *mut GcHeader,
    finobjrold: *mut GcHeader,
    /// `GCmajorminor`: live bytes after the last major collection.
    majorminor: usize,
    /// Finalizers are run by the caller after the collection, not from inside a step
    /// (`State::collect`); otherwise a step reports [`StepOutcome::NeedFin`].
    defer_fin: bool,
    /// 5.4 `GCestimate`: estimate of the live bytes after the last cycle (base of `setpause`, of
    /// the major-collection test of `genstep`).
    #[cfg(rustynail_lua54)]
    pub(crate) estimate: usize,
    /// 5.4 `lastatomic`: work of the last atomic phase if the last major collection was "bad"
    /// (0 otherwise); non-zero keeps the collector in `stepgenfull` mode.
    #[cfg(rustynail_lua54)]
    pub(crate) lastatomic: usize,
    /// 5.4 work units (`propagatemark`/`remarkupvals`/`markbeingfnz`) accumulated by `atomic`.
    #[cfg(rustynail_lua54)]
    pub(crate) work54: usize,
    /// 5.4: objects visited by the last `sweep_some` call (`sweeplist`'s `countout`).
    #[cfg(rustynail_lua54)]
    pub(crate) swept54: usize,
    /// 5.4 GC stress 2 in generational mode: steps taken (every `STRESS_MAJOR`th is a major).
    #[cfg(rustynail_lua54)]
    pub(crate) stress_count: u32,
}

impl GcState {
    pub(crate) fn new() -> Self {
        let null = std::ptr::null_mut();
        GcState {
            phase: GcPhase::Pause,
            gray: null,
            grayagain: null,
            weak: null,
            allweak: null,
            ephemeron: null,
            finobj: null,
            tobefnz: null,
            fixedgc: null,
            marked: 0,
            threshold: 0,
            last_gap: 0,
            params: GcParams::defaults(),
            stp: 0,
            emergency: false,
            stress: std::env::var_os("RUSTYNAIL_GC_STRESS")
                .is_some_and(|v| v != "0" && v != "2" && !v.is_empty()),
            stress_steps: std::env::var_os("RUSTYNAIL_GC_STRESS").is_some_and(|v| v == "2"),
            #[cfg(feature = "jit_trace")]
            virt_mark: 0,
            #[cfg(feature = "jit_trace")]
            watch: Vec::new(),
            #[cfg(feature = "jit_trace")]
            watch_hit: false,
            verify_steps: std::env::var_os("RUSTYNAIL_GC_VERIFY").is_some_and(|v| v == "1"),
            in_collect: false,
            kind: GcKind::Incremental,
            fin_hook: None,
            twups: null.cast(),
            collections: 0,
            sweep_prev: null,
            survival: null,
            old1: null,
            reallyold: null,
            firstold1: null,
            finobjsur: null,
            finobjold1: null,
            finobjrold: null,
            majorminor: 0,
            defer_fin: false,
            #[cfg(rustynail_lua54)]
            estimate: 0,
            #[cfg(rustynail_lua54)]
            lastatomic: 0,
            #[cfg(rustynail_lua54)]
            work54: 0,
            #[cfg(rustynail_lua54)]
            swept54: 0,
            #[cfg(rustynail_lua54)]
            stress_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// colour helpers
// ---------------------------------------------------------------------------------------------

#[inline]
pub(crate) unsafe fn iswhite(o: *const GcHeader) -> bool {
    // SAFETY: G1.
    unsafe { (*o).marked & WHITEBITS != 0 }
}
#[inline]
unsafe fn isgray(o: *const GcHeader) -> bool {
    // SAFETY: G1.
    unsafe { (*o).marked & MASKCOLORS == 0 }
}
#[inline]
unsafe fn tofinalize(o: *const GcHeader) -> bool {
    // SAFETY: G1.
    unsafe { (*o).marked & FINALIZEDBIT != 0 }
}
#[inline]
unsafe fn set2gray(o: *mut GcHeader) {
    // SAFETY: G1.
    unsafe { (*o).marked &= !MASKCOLORS }
}
/// `nw2black`: a non-white object becomes black.
#[inline]
pub(crate) unsafe fn nw2black(o: *mut GcHeader) {
    // SAFETY: G1.
    unsafe { (*o).marked |= BLACKBIT }
}
#[inline]
unsafe fn set2black(o: *mut GcHeader) {
    // SAFETY: G1.
    unsafe { (*o).marked = ((*o).marked & !WHITEBITS) | BLACKBIT }
}

/// Object lists swept by the collector.
#[derive(Clone, Copy)]
enum ObjList {
    AllGc,
    FinObj,
    ToBeFnz,
}

/// Which gray-ish list to link a table into.
#[derive(Clone, Copy)]
enum List {
    Gray,
    GrayAgain,
    Weak,
    AllWeak,
    Ephemeron,
}

// ---------------------------------------------------------------------------------------------
// per-kind dispatch (B3: add kinds here)
// ---------------------------------------------------------------------------------------------

/// `objsize`.
unsafe fn objsize(o: *const GcHeader) -> usize {
    // SAFETY: G1.
    unsafe {
        match (*o).tt {
            LUA_VSHRSTR | LUA_VLNGSTR => {
                let s = o.cast::<LuaString>();
                counted_size((*s).kind(), (*s).len()).unwrap_or(0)
            }
            LUA_VTABLE => (*o.cast::<Table>()).byte_size(),
            LUA_VLCL => size_lclosure((*o.cast::<LClosure>()).nupvalues as usize),
            LUA_VCCL => size_cclosure((*o.cast::<CClosure>()).nupvalues as usize),
            LUA_VUPVAL => std::mem::size_of::<UpVal>(),
            LUA_VPROTO => (*o.cast::<Proto>()).byte_size(),
            LUA_VUSERDATA => {
                let u = o.cast::<Udata>();
                size_udata((*u).nuvalue as usize, (*u).len).unwrap_or(0)
            }
            LUA_VTHREAD => thread_size(o.cast::<Thread>()),
            t => {
                debug_assert!(false, "objsize: unsupported object kind {t:#x}");
                0
            }
        }
    }
}

/// `getgclist`: the intrusive gray-list link of an object kind that has one.
unsafe fn getgclist(o: *mut GcHeader) -> *mut *mut GcHeader {
    // SAFETY: G1.
    unsafe {
        match (*o).tt {
            LUA_VTABLE => &raw mut (*o.cast::<Table>()).gclist,
            LUA_VLCL => &raw mut (*o.cast::<LClosure>()).gclist,
            LUA_VCCL => &raw mut (*o.cast::<CClosure>()).gclist,
            LUA_VTHREAD => &raw mut (*o.cast::<Thread>()).gclist,
            LUA_VPROTO => &raw mut (*o.cast::<Proto>()).gclist,
            LUA_VUSERDATA => &raw mut (*o.cast::<Udata>()).gclist,
            t => {
                debug_assert!(false, "getgclist: kind {t:#x} has no gclist");
                std::ptr::null_mut()
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Marker
// ---------------------------------------------------------------------------------------------

/// Handle given to root-marking callbacks.
pub struct Marker<'a> {
    heap: &'a mut Heap,
}

impl Marker<'_> {
    /// Mark the object a value refers to (no-op for non-collectable values).
    pub fn mark_value(&mut self, v: &Value) {
        self.heap.markvalue(v);
    }
    /// Mark an object.
    ///
    /// # Safety
    /// `o` must point to a live object of the heap being collected.
    pub unsafe fn mark_object(&mut self, o: *mut GcHeader) {
        // SAFETY: caller contract.
        unsafe { self.heap.markobject(o) }
    }
    pub fn mark_table(&mut self, t: TableRef) {
        // SAFETY: a `TableRef` points to a live table.
        unsafe { self.heap.markobject(t.as_ptr().cast()) }
    }
    pub fn mark_str(&mut self, s: StrRef) {
        // SAFETY: a `StrRef` points to a live string.
        unsafe { self.heap.markobject(s.as_ptr().cast()) }
    }
}

// ---------------------------------------------------------------------------------------------
// Heap: collector implementation
// ---------------------------------------------------------------------------------------------

impl Heap {
    // ---- barriers -----------------------------------------------------------------------------

    /// `keepinvariant`: the "no black points to white" invariant must hold (marking phases).
    #[inline]
    pub(crate) fn keepinvariant(&self) -> bool {
        matches!(self.gc.phase, GcPhase::Propagate | GcPhase::Atomic)
    }

    /// `makewhite`: repaint with the current white.
    #[inline]
    unsafe fn makewhite(&self, o: *mut GcHeader) {
        // SAFETY: G1.
        unsafe { (*o).marked = ((*o).marked & !MASKCOLORS) | (self.current_white & WHITEBITS) }
    }

    /// `luaC_barrier`: black object `o` now points to white object `v` (forward barrier).
    // Barrier arguments are always live objects of this heap (invariant G1), like every `Gc<T>`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[inline]
    pub fn barrier(&mut self, o: *mut GcHeader, v: &Value) {
        if let Some(g) = v.as_gc() {
            // SAFETY: both are live objects of this heap (callers' contract).
            unsafe { self.barrier_obj(o, g) }
        }
    }

    /// `luaC_objbarrier`: same as [`Heap::barrier`] for an object pointer.
    ///
    /// # Safety
    /// `o` and `v` must be live objects of this heap.
    #[inline]
    pub unsafe fn barrier_obj(&mut self, o: *mut GcHeader, v: *mut GcHeader) {
        // SAFETY: caller contract (G1).
        unsafe {
            if (*o).marked & BLACKBIT != 0 && iswhite(v) {
                self.barrier_slow(o, v);
            }
        }
    }

    #[cold]
    unsafe fn barrier_slow(&mut self, o: *mut GcHeader, v: *mut GcHeader) {
        // SAFETY: G1.
        unsafe {
            if self.keepinvariant() {
                self.reallymark(v); // restore the invariant: mark the new target
                if isold(o) {
                    debug_assert!(!isold(v), "a white object cannot be old");
                    setage(v, G_OLD0); // restore the generational invariant
                }
            } else if self.gc.kind != GcKind::GenMinor {
                // incremental sweep phase: sweep `o` now to avoid further barriers on it
                self.makewhite(o);
            }
        }
    }

    /// `luaC_barrierback`: black object `o` (a table/userdata) was assigned the value `v` and may
    /// now point to a white object: turn it gray again and revisit it in the atomic phase.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // see `barrier`
    #[inline]
    pub fn barrier_back(&mut self, o: *mut GcHeader, v: &Value) {
        if let Some(g) = v.as_gc() {
            // SAFETY: `o`/`g` are live objects of this heap (G1).
            unsafe {
                if (*o).marked & BLACKBIT != 0 && iswhite(g) {
                    self.barrier_back_slow(o);
                }
            }
        }
    }

    #[cold]
    unsafe fn barrier_back_slow(&mut self, o: *mut GcHeader) {
        // SAFETY: G1; `o` is a black object with a `gclist` (table, userdata).
        unsafe {
            if getage(o) == G_TOUCHED2 {
                set2gray(o); // already in a gray list: becomes touched1
            } else {
                self.linkgclist(o, List::GrayAgain); // also paints it gray
            }
            if isold(o) {
                setage(o, G_TOUCHED1); // touched in the current cycle
            }
        }
    }

    // ---- configuration ------------------------------------------------------------------------

    /// Enable/disable GC stress mode (every `check_gc` collects).
    pub fn set_gc_stress(&mut self, on: bool) {
        self.gc.stress = on;
    }
    pub fn gc_stress(&self) -> bool {
        self.gc.stress
    }
    /// `g->gcstp & GCSTPGC`: the collector is stopped internally (running a finalizer, building
    /// the state): `lua_gc` refuses every request with -1.
    pub fn gc_internally_stopped(&self) -> bool {
        self.gc.stp & GCSTP_GC != 0
    }
    /// Stop the collector (`collectgarbage("stop")`).
    pub fn gc_stop(&mut self) {
        self.gc.stp |= GCSTP_USER;
    }
    pub fn gc_restart(&mut self) {
        self.gc.stp &= !GCSTP_USER;
        #[cfg(not(rustynail_lua54))]
        {
            self.gc.threshold = 0;
        }
        // 5.4: `luaE_setdebt(g, 0)` (the next allocation makes a step due)
        #[cfg(rustynail_lua54)]
        self.set_debt54(0);
    }
    pub fn gc_is_running(&self) -> bool {
        self.gc.stp == 0
    }
    pub fn gc_params(&mut self) -> &mut GcParams {
        &mut self.gc.params
    }
    /// The collector mode (`g->gckind`).
    pub fn gc_kind(&self) -> GcKind {
        self.gc.kind
    }
    /// Force the next safepoint to run a step (`luaE_setdebt(g, 0)`).
    pub fn gc_set_threshold(&mut self, threshold: usize) {
        self.gc.threshold = threshold;
    }
    /// Install the hook that runs `__gc` metamethods (task B3 supplies the real one).
    pub fn set_finalizer_hook(&mut self, hook: Option<FinalizerHook>) {
        self.gc.fin_hook = hook;
    }
    /// Bytes at which the next safepoint will start a collection.
    pub fn gc_threshold(&self) -> usize {
        self.gc.threshold
    }
    pub fn gc_phase(&self) -> GcPhase {
        self.gc.phase
    }
    /// Completed collection cycles.
    pub fn gc_cycles(&self) -> u64 {
        self.gc.collections
    }
    /// Pending finalizer count (`tobefnz` length).
    pub fn pending_finalizers(&self) -> usize {
        let mut n = 0;
        let mut p = self.gc.tobefnz;
        while !p.is_null() {
            n += 1;
            // SAFETY: G1/G3.
            p = unsafe { (*p).next };
        }
        n
    }

    /// `luaC_fix`: make the newest object (head of `allgc`) permanent.
    pub(crate) fn fix_object(&mut self, o: *mut GcHeader) {
        debug_assert_eq!(self.allgc, o);
        // SAFETY: G1; `o` is the head of allgc.
        unsafe {
            set2gray(o);
            setage(o, G_OLD); // gray and old forever
            self.allgc = (*o).next;
            (*o).next = self.gc.fixedgc;
            self.gc.fixedgc = o;
        }
    }

    // ---- safepoint -------------------------------------------------------------------------------

    /// The GC safepoint (`luaC_checkGC`) for callers without a `State`: if the allocation
    /// threshold was crossed (or always, in stress mode) perform one incremental step (finalizers
    /// run through the installed hook). Returns whether a step ran.
    #[inline]
    pub fn check_gc(&mut self, roots: &mut dyn FnMut(&mut Marker)) -> bool {
        if !self.gc_due() {
            return false;
        }
        if self.gc.stress {
            self.full_gc(roots, false, false);
            return true;
        }
        #[cfg(rustynail_lua54)]
        gen54::luac_step54(&mut gen54::HeapHost { heap: self, roots });
        #[cfg(not(rustynail_lua54))]
        {
            if self.gc.kind == GcKind::GenMinor {
                self.gen_step(roots);
                self.call_all_pending_finalizers();
                return true;
            }
            let mut budget = self.step_budget();
            loop {
                match self.inc_step(roots, &mut budget) {
                    StepOutcome::Done => break,
                    StepOutcome::NeedFin => {
                        self.call_one_pending_finalizer();
                        budget -= CWUFIN;
                        if budget <= 0 {
                            self.finish_step();
                            break;
                        }
                    }
                }
            }
        }
        true
    }

    /// Run a full stop-the-world collection (`luaC_fullgc`). Pending finalizers are run through the
    /// installed hook (if any) at the end.
    pub fn collect(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        self.full_gc(roots, false, false)
    }

    /// Like [`Heap::collect`] but leaves the pending finalizers queued: the caller (`State`) runs
    /// them with [`Heap::next_finalizable`] outside of any heap borrow.
    pub fn collect_no_finalizers(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        self.full_gc(roots, false, true)
    }

    /// J5 sinking (TR29.4): charge `n` bytes for a sunk allocation that never happened (counted in the total like an
    /// allocation, not against the memory limit).
    #[cfg(feature = "jit_trace")]
    pub fn charge_virtual(&mut self, n: usize) {
        self.mem.charge_virtual(n);
    }
    /// J5 sinking: credit back up to `n` virtual bytes.
    #[cfg(feature = "jit_trace")]
    pub fn uncharge_virtual(&mut self, n: usize) {
        self.mem.uncharge_virtual(n);
    }
    /// J5 recorder (F8): from now until [`Heap::unwatch_all`], freeing `o` sets the flag read by [`Heap::watch_hit`]. The
    /// collector's behaviour is not changed (the object is not kept alive): the recorder aborts instead of reading a freed
    /// table, so the GC-observable behaviour of a run stays that of the interpreter (TR29.6).
    #[cfg(feature = "jit_trace")]
    pub fn watch_object(&mut self, o: *const GcHeader) {
        self.gc.watch.push(o as usize);
    }
    /// J5 recorder: forgets every watched object and the hit flag.
    #[cfg(feature = "jit_trace")]
    pub fn unwatch_all(&mut self) {
        self.gc.watch.clear();
        self.gc.watch_hit = false;
    }
    /// J5 recorder: was a watched object freed since [`Heap::unwatch_all`]?
    #[cfg(feature = "jit_trace")]
    pub fn watch_hit(&self) -> bool {
        self.gc.watch_hit
    }
    /// J5 sinking: virtual bytes currently charged.
    #[cfg(feature = "jit_trace")]
    pub fn virtual_bytes(&self) -> usize {
        self.mem.virtual_bytes()
    }
    /// J5 sinking: the address of the virtual-byte counter, for the trace code's in-place charge (P-L: one cell).
    #[cfg(feature = "jit_trace")]
    pub fn virtual_cell(&mut self) -> *mut usize {
        self.mem.virtual_cell()
    }
    /// The bytes of real objects (the total without the virtual bytes of J5 sinking, which are dead): the base of the 5.4
    /// `GCestimate` (live bytes), so that the pause threshold is not inflated by garbage that was never allocated.
    #[inline(always)]
    #[cfg(rustynail_lua54)]
    pub(crate) fn real_total(&self) -> usize {
        #[cfg(feature = "jit_trace")]
        {
            self.mem
                .total_bytes()
                .saturating_sub(self.mem.virtual_bytes())
        }
        #[cfg(not(feature = "jit_trace"))]
        {
            self.mem.total_bytes()
        }
    }
    /// The virtual bytes charged before the current cycle started are credited back when it ends.
    #[inline(always)]
    pub(crate) fn jit_uncharge_cycle(&mut self) {
        #[cfg(feature = "jit_trace")]
        {
            let m = std::mem::take(&mut self.gc.virt_mark);
            self.mem.uncharge_virtual(m);
        }
    }
    /// A full or young collection frees every sunk (dead) allocation: all virtual bytes are credited back.
    #[inline(always)]
    pub(crate) fn jit_uncharge_all(&mut self) {
        #[cfg(feature = "jit_trace")]
        {
            self.gc.virt_mark = 0;
            let v = self.mem.virtual_bytes();
            self.mem.uncharge_virtual(v);
        }
    }

    /// Is a collection due at a safepoint (threshold crossed, or stress mode, and not stopped)?
    #[inline]
    pub fn gc_due(&self) -> bool {
        self.gc.stp == 0
            && (self.gc.stress
                || self.gc.stress_steps
                || self.mem.total_bytes() >= self.gc.threshold)
    }
    /// P-B (allocation storm guard): about how many bytes the program allocates between two collector steps in the current mode
    /// (generational: the minor-collection debt, else the incremental step size). A trace that allocates a string in every iteration
    /// exits at each allocation when this is not much larger than what one iteration allocates (the helper never allocates when a
    /// step would be due behind it), which costs more than tier 1 runs the loop.
    pub fn jit_gc_step_hint(&self) -> usize {
        if self.gc.last_gap != 0 {
            return self.gc.last_gap;
        }
        if self.gc.kind == GcKind::GenMinor {
            #[cfg(not(rustynail_lua54))]
            return self.minor_debt_bytes();
            #[cfg(rustynail_lua54)]
            return (self.mem.total_bytes() / 100)
                .saturating_mul(self.gc.params.v54.genminormul as usize);
        }
        self.step_size_bytes()
    }
    /// Remembers the room the collector left before the next step (called at the end of `State::gc_step`).
    pub fn note_step_gap(&mut self) {
        self.gc.last_gap = self
            .gc
            .threshold
            .saturating_sub(self.mem.total_bytes())
            .max(1);
    }
    /// Is a stress mode on (a step at every safepoint)?
    pub fn jit_gc_stress(&self) -> bool {
        self.gc.stress || self.gc.stress_steps
    }
    /// J5 class-A helpers (TR1): would allocating `extra` more bytes neither fail on the memory limit nor make a
    /// collection due at the interpreter's `check_gc` right after the allocation (`gc_due` after the bytes are counted)?
    /// A trace allocates only when this holds, so it never has to run a GC step or raise an error itself.
    #[inline]
    pub fn jit_alloc_ok(&self, extra: usize) -> bool {
        let idle = self.gc.stp != 0
            || (!self.gc.stress
                && !self.gc.stress_steps
                && self.mem.total_bytes().saturating_add(extra) < self.gc.threshold);
        idle && self.mem.would_fit(extra)
    }
    /// Is a collection running right now?
    pub fn gc_in_collect(&self) -> bool {
        self.gc.in_collect
    }
    /// `gcstp` bits.
    pub fn gc_stp(&self) -> u8 {
        self.gc.stp
    }
    pub fn gc_set_stp(&mut self, v: u8) {
        self.gc.stp = v;
    }
    /// Are finalizers waiting to run?
    pub fn has_pending_finalizers(&self) -> bool {
        !self.gc.tobefnz.is_null()
    }
    /// `udata2finalize`: take the next object to finalize back to the normal list and return it.
    /// Must only be called when [`Heap::has_pending_finalizers`].
    pub fn next_finalizable(&mut self) -> *mut GcHeader {
        self.udata2finalize()
    }
    /// Start closing the state (`luaC_freeallobjects` up to the finalizer calls): no new finalizers
    /// get registered and every object with a finalizer is queued.
    pub fn gc_begin_close(&mut self) {
        self.gc.stp = GCSTP_CLOSING;
        self.leave_generational();
        self.separatetobefnz(true);
    }
    /// Make an arbitrary (already allocated) object permanent: `luaC_fix` for objects that are not at
    /// the head of `allgc` (e.g. an interned string that existed before).
    pub(crate) fn fix_any(&mut self, o: *mut GcHeader) {
        // SAFETY: G1 — `o` is on `allgc`; we search for its predecessor link.
        unsafe {
            if (*o).marked & FINALIZEDBIT != 0 || self.fixed_contains(o) {
                return;
            }
            let mut p: *mut *mut GcHeader = &raw mut self.allgc;
            while *p != o {
                if (*p).is_null() {
                    return; // not on allgc (already fixed)
                }
                p = &raw mut (**p).next;
            }
            *p = (*o).next;
            set2gray(o);
            setage(o, G_OLD);
            (*o).next = self.gc.fixedgc;
            self.gc.fixedgc = o;
        }
    }
    fn fixed_contains(&self, o: *mut GcHeader) -> bool {
        let mut p = self.gc.fixedgc;
        while !p.is_null() {
            if p == o {
                return true;
            }
            // SAFETY: G1.
            p = unsafe { (*p).next };
        }
        false
    }

    /// Emergency collection (allocation failure): like `collect` but no finalizers run and the string
    /// table is not shrunk.
    pub fn collect_emergency(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        self.full_gc(roots, true, true)
    }

    // ---- the incremental state machine ----------------------------------------------------------

    /// `restartcollection`: start marking from the roots.
    fn restart_collection(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        self.gc.gray = std::ptr::null_mut();
        self.gc.grayagain = std::ptr::null_mut();
        self.gc.weak = std::ptr::null_mut();
        self.gc.allweak = std::ptr::null_mut();
        self.gc.ephemeron = std::ptr::null_mut();
        self.gc.marked = 0;
        roots(&mut Marker { heap: self });
        self.markbeingfnz();
    }

    /// `entersweep`: start sweeping `allgc` (every surviving object is repainted white).
    fn entersweep(&mut self) {
        self.gc.phase = GcPhase::SweepAllGc;
        self.gc.sweep_prev = std::ptr::null_mut();
    }

    /// Work done by one sweep step (`GCSWEEPMAX`; 100 in 5.4).
    const SWEEPMAX: usize = if cfg!(rustynail_lua54) { 100 } else { 20 };

    /// `sweepstep`: sweep (at most) `SWEEPMAX` objects of the list of the current phase (all of it if
    /// `fast`); move to `next` when the list is exhausted.
    fn sweepstep(&mut self, list: ObjList, next: GcPhase, fast: bool) {
        let max = if fast { usize::MAX } else { Self::SWEEPMAX };
        // 5.4: `GCestimate` follows the freed memory
        #[cfg(rustynail_lua54)]
        let before = self.mem.total_bytes();
        let done = self.sweep_some(list, max);
        #[cfg(rustynail_lua54)]
        {
            let freed = before - self.mem.total_bytes();
            self.gc.estimate = self.gc.estimate.saturating_sub(freed);
        }
        if done {
            self.gc.phase = next;
            self.gc.sweep_prev = std::ptr::null_mut();
        }
    }

    /// One incremental unit of the collector. `fast` skips incremental propagation (everything is
    /// traversed by the atomic phase) and sweeps whole lists.
    fn singlestep(&mut self, roots: &mut dyn FnMut(&mut Marker), fast: bool) -> Step {
        debug_assert!(!self.gc.in_collect, "collector is not reentrant");
        self.gc.in_collect = true;
        let r = match self.gc.phase {
            GcPhase::Pause => {
                #[cfg(feature = "jit_trace")]
                {
                    self.gc.virt_mark = self.mem.virtual_bytes();
                }
                self.restart_collection(roots);
                self.gc.phase = GcPhase::Propagate;
                Step::Work(1)
            }
            GcPhase::Propagate => {
                if fast || self.gc.gray.is_null() {
                    self.gc.phase = GcPhase::Atomic; // finish propagate phase
                    Step::Work(if cfg!(rustynail_lua54) { 0 } else { 1 })
                } else {
                    Step::Work(self.propagatemark())
                }
            }
            #[cfg(rustynail_lua54)]
            GcPhase::Atomic => {
                // 5.4: the work is what `atomic` traversed; the estimate starts at the heap size
                self.gc.work54 = 0;
                self.atomic(roots);
                self.entersweep();
                self.gc.estimate = self.real_total();
                Step::Work(self.gc.work54)
            }
            #[cfg(not(rustynail_lua54))]
            GcPhase::Atomic => {
                self.atomic(roots);
                if self.check_major_minor() {
                    Step::Minor // back to generational mode
                } else {
                    self.entersweep();
                    Step::Atomic
                }
            }
            GcPhase::SweepAllGc => {
                self.sweepstep(ObjList::AllGc, GcPhase::SweepFinObj, fast);
                Step::Work(self.swept_work())
            }
            GcPhase::SweepFinObj => {
                self.sweepstep(ObjList::FinObj, GcPhase::SweepToBeFnz, fast);
                Step::Work(self.swept_work())
            }
            GcPhase::SweepToBeFnz => {
                self.sweepstep(ObjList::ToBeFnz, GcPhase::SweepEnd, fast);
                Step::Work(self.swept_work())
            }
            GcPhase::SweepEnd => {
                self.check_sizes();
                self.gc.phase = GcPhase::CallFin;
                Step::Work(if cfg!(rustynail_lua54) {
                    0
                } else {
                    Self::SWEEPMAX
                })
            }
            GcPhase::CallFin => {
                if !self.gc.tobefnz.is_null() && !self.gc.emergency && !self.gc.defer_fin {
                    Step::NeedFin
                } else {
                    self.gc.phase = GcPhase::Pause;
                    self.gc.collections += 1;
                    Step::Paused
                }
            }
        };
        self.gc.in_collect = false;
        r
    }

    /// Work of a sweep step: `GCSWEEPMAX` (5.4: the number of objects visited).
    #[inline(always)]
    fn swept_work(&self) -> usize {
        #[cfg(rustynail_lua54)]
        {
            self.gc.swept54
        }
        #[cfg(not(rustynail_lua54))]
        {
            Self::SWEEPMAX
        }
    }

    /// `luaC_runtilstate`: run steps until `target`. Stops early (`NeedFin`) when finalizers must be
    /// called by the caller.
    pub(crate) fn run_until(
        &mut self,
        target: GcPhase,
        roots: &mut dyn FnMut(&mut Marker),
    ) -> StepOutcome {
        while self.gc.phase != target {
            if let Step::NeedFin = self.singlestep(roots, true) {
                return StepOutcome::NeedFin;
            }
        }
        StepOutcome::Done
    }

    /// `setpause`: wait until memory has grown by the `pause` parameter before the next cycle.
    fn set_pause(&mut self) {
        self.jit_uncharge_cycle();
        #[cfg(rustynail_lua54)]
        {
            self.set_pause54();
        }
        #[cfg(not(rustynail_lua54))]
        {
            self.set_pause55();
        }
    }
    #[cfg(not(rustynail_lua54))]
    #[inline(always)]
    fn set_pause55(&mut self) {
        self.gc.threshold = self
            .gc
            .params
            .pause(self.gc.marked)
            // (5.4: a debt clamped to 0 is not due; the next allocation makes it due)
            .max(self.mem.total_bytes() + cfg!(rustynail_lua54) as usize);
    }

    /// Bytes of "debt" between two incremental steps (`applygcparam(STEPSIZE, 100)`).
    fn step_size_bytes(&self) -> usize {
        self.gc.params.stepsize(100).max(1)
    }

    /// Number of collector steps the reference would have taken while `bytes` were allocated
    /// through a long run of safepoints (one per `stepsize` bytes), e.g. while parsing.
    pub fn steps_for_alloc(&self, bytes: usize) -> usize {
        bytes / self.step_size_bytes()
    }

    /// Make the next safepoint run a step (the step itself sets the next threshold).
    pub fn force_due(&mut self) {
        // (zero debt, not "the whole heap": the 5.4 step size depends on the debt)
        self.gc.threshold = self.mem.total_bytes();
    }

    /// Work units for one step (`applygcparam(STEPMUL, stepsize / sizeof(void*))`).
    pub fn step_budget(&self) -> isize {
        if self.gc.stress_steps {
            return 1;
        }
        // 5.4 `incstep`: pay the debt (bytes allocated past the threshold) plus one step size,
        // both converted to work units (`/ WORK2MEM * stepmul`)
        #[cfg(rustynail_lua54)]
        {
            let stepmul = (GcParams::get54(self.gc.params.v54.stepmul) | 1) as isize;
            let debt = self.mem.total_bytes() as isize - self.gc.threshold as isize;
            let stepsize = self.step_size_bytes() as isize;
            (debt.max(0) / 16)
                .saturating_mul(stepmul)
                .saturating_add((stepsize / 16).saturating_mul(stepmul))
        }
        #[cfg(not(rustynail_lua54))]
        {
            self.gc.params.stepmul(self.step_size_bytes() / 8) as isize
        }
    }

    /// End of a step: pause if the cycle is over, otherwise schedule the next step.
    pub(crate) fn finish_step(&mut self) {
        if self.gc.phase == GcPhase::Pause {
            self.set_pause();
        } else {
            self.gc.threshold = self.mem.total_bytes() + self.step_size_bytes();
        }
    }

    /// `incstep`: perform steps until `budget` work units are spent, a cycle ends or the atomic
    /// phase ran. `NeedFin` means the caller has to run (at least) one finalizer and call again
    /// (or `Heap::finish_step`).
    pub fn inc_step(
        &mut self,
        roots: &mut dyn FnMut(&mut Marker),
        budget: &mut isize,
    ) -> StepOutcome {
        let fast = *budget <= 0 && !self.gc.stress_steps;
        loop {
            match self.singlestep(roots, fast) {
                Step::NeedFin => return StepOutcome::NeedFin,
                Step::Paused => break,
                Step::Minor => return StepOutcome::Done, // debt already set by `atomic2gen`
                // (5.4's `incstep` does not stop after the atomic step)
                Step::Atomic if !fast && !cfg!(rustynail_lua54) => break,
                Step::Atomic => {}
                Step::Work(w) => *budget -= w as isize,
            }
            if !fast && *budget <= 0 {
                break;
            }
        }
        self.finish_step();
        if self.gc.verify_steps {
            self.verify();
        }
        StepOutcome::Done
    }

    /// Run one pending finalizer through the installed hook.
    fn call_one_pending_finalizer(&mut self) {
        if self.gc.fin_hook.is_none() || self.gc.tobefnz.is_null() {
            // nothing can run them: drop the request so the cycle can end
            self.gc.defer_fin = true;
            return;
        }
        let mut hook = self.gc.fin_hook.take();
        let oldstp = self.gc.stp;
        self.gc.stp |= GCSTP_GC;
        let o = self.udata2finalize();
        if let Some(h) = hook.as_mut() {
            h(self, o);
        }
        self.gc.stp = oldstp;
        if self.gc.fin_hook.is_none() {
            self.gc.fin_hook = hook;
        }
    }

    /// `luaC_fullgc` (`fullinc`): complete collection. With `defer_fin` pending finalizers stay
    /// queued for the caller; otherwise they run through the installed hook.
    fn full_gc(&mut self, roots: &mut dyn FnMut(&mut Marker), emergency: bool, defer_fin: bool) {
        if self.gc.in_collect {
            return;
        }
        self.jit_uncharge_all();
        #[cfg(rustynail_lua54)]
        {
            self.gc.defer_fin = defer_fin;
            gen54::fullgc54(&mut gen54::HeapHost { heap: self, roots }, emergency);
            self.gc.defer_fin = false;
        }
        #[cfg(not(rustynail_lua54))]
        self.full_gc55(roots, emergency, defer_fin);
    }

    #[cfg(not(rustynail_lua54))]
    #[inline(always)]
    fn full_gc55(&mut self, roots: &mut dyn FnMut(&mut Marker), emergency: bool, defer_fin: bool) {
        self.gc.emergency = emergency;
        self.gc.defer_fin = defer_fin;
        match self.gc.kind {
            GcKind::GenMinor => self.fullgen(roots),
            kind => {
                // a major collection of generational mode is a full incremental one
                self.gc.kind = GcKind::Incremental;
                self.fullinc(roots);
                self.gc.kind = kind;
            }
        }
        self.gc.emergency = false;
        self.gc.defer_fin = false;
    }

    /// `fullinc`: a full collection in incremental mode.
    #[cfg(not(rustynail_lua54))]
    fn fullinc(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        if self.keepinvariant() {
            self.entersweep(); // sweep everything to turn black objects back to white
        }
        for target in [GcPhase::Pause, GcPhase::CallFin, GcPhase::Pause] {
            while self.run_until(target, roots) == StepOutcome::NeedFin {
                self.call_one_pending_finalizer();
            }
        }
        self.set_pause();
    }

    // ---- marking -----------------------------------------------------------------------------------

    fn markvalue(&mut self, v: &Value) {
        if let Some(o) = v.as_gc() {
            // SAFETY: V1.
            unsafe { self.markobject(o) }
        }
    }

    /// `markobject`.
    unsafe fn markobject(&mut self, o: *mut GcHeader) {
        // SAFETY: G1.
        unsafe {
            if iswhite(o) {
                self.reallymark(o);
            }
        }
    }

    unsafe fn markobject_n(&mut self, o: *mut GcHeader) {
        if !o.is_null() {
            // SAFETY: forwarded.
            unsafe { self.markobject(o) }
        }
    }

    /// `reallymarkobject`.
    unsafe fn reallymark(&mut self, o: *mut GcHeader) {
        // SAFETY: G1.
        unsafe {
            // `objsize` per kind, in the arm that handles the kind (one type dispatch per object)
            match (*o).tt {
                LUA_VSHRSTR | LUA_VLNGSTR => {
                    let s = o.cast::<LuaString>();
                    self.gc.marked += counted_size((*s).kind(), (*s).len()).unwrap_or(0);
                    set2black(o)
                }
                LUA_VUPVAL => {
                    self.gc.marked += std::mem::size_of::<UpVal>();
                    let uv = o.cast::<UpVal>();
                    if Gc::from_raw(uv).is_open() {
                        set2gray(o); // open upvalues are kept gray
                    } else {
                        set2black(o); // closed upvalues are visited here
                    }
                    let v = *(*uv).v;
                    self.markvalue(&v);
                }
                LUA_VUSERDATA => {
                    let u = o.cast::<Udata>();
                    self.gc.marked += size_udata((*u).nuvalue as usize, (*u).len).unwrap_or(0);
                    if (*u).nuvalue == 0 {
                        self.markobject_n((*u).metatable.cast());
                        set2black(o);
                    } else {
                        self.linkgclist(o, List::Gray);
                    }
                }
                LUA_VTABLE => {
                    self.gc.marked += (*o.cast::<Table>()).byte_size();
                    self.linkgclist(o, List::Gray)
                }
                LUA_VLCL => {
                    self.gc.marked += size_lclosure((*o.cast::<LClosure>()).nupvalues as usize);
                    self.linkgclist(o, List::Gray)
                }
                LUA_VCCL | LUA_VTHREAD | LUA_VPROTO => {
                    self.gc.marked += objsize(o);
                    self.linkgclist(o, List::Gray)
                }
                t => debug_assert!(false, "reallymark: unsupported kind {t:#x}"),
            }
        }
    }

    unsafe fn linkgclist(&mut self, o: *mut GcHeader, which: List) {
        let list: *mut *mut GcHeader = match which {
            List::Gray => &raw mut self.gc.gray,
            List::GrayAgain => &raw mut self.gc.grayagain,
            List::Weak => &raw mut self.gc.weak,
            List::AllWeak => &raw mut self.gc.allweak,
            List::Ephemeron => &raw mut self.gc.ephemeron,
        };
        // SAFETY: G1; `o` is not in a gray list (not gray).
        unsafe {
            debug_assert!(!isgray(o));
            *getgclist(o) = *list;
            *list = o;
            set2gray(o);
        }
    }

    fn markbeingfnz(&mut self) {
        let mut o = self.gc.tobefnz;
        while !o.is_null() {
            #[cfg(rustynail_lua54)]
            {
                self.gc.work54 += 1;
            }
            // SAFETY: G1/G3.
            unsafe {
                self.markobject(o);
                o = (*o).next;
            }
        }
    }

    // ---- traversal -----------------------------------------------------------------------------------

    fn propagateall(&mut self) {
        while !self.gc.gray.is_null() {
            #[cfg(not(rustynail_lua54))]
            self.propagatemark();
            #[cfg(rustynail_lua54)]
            {
                self.gc.work54 += self.propagatemark();
            }
        }
    }

    /// Traverse one gray object; returns the work done (roughly: its size in slots).
    fn propagatemark(&mut self) -> usize {
        let o = self.gc.gray;
        // SAFETY: G1; `o` is gray and on the gray list.
        unsafe {
            set2black(o);
            self.gc.gray = *getgclist(o);
            // work = `objsize / 16`, computed per kind next to the traversal (one type dispatch;
            // the size fields sit on the object's first cache line, which the traversal loads)
            let size = match (*o).tt {
                LUA_VTABLE => {
                    let sz = (*o.cast::<Table>()).byte_size();
                    self.traversetable(o.cast());
                    sz
                }
                LUA_VLCL => {
                    let sz = size_lclosure((*o.cast::<LClosure>()).nupvalues as usize);
                    self.traverse_lclosure(o.cast());
                    sz
                }
                LUA_VUSERDATA => {
                    let sz = objsize(o);
                    self.traverse_udata(o.cast());
                    sz
                }
                LUA_VCCL => {
                    let sz = objsize(o);
                    self.traverse_cclosure(o.cast());
                    sz
                }
                LUA_VPROTO => {
                    let sz = objsize(o);
                    self.traverse_proto(o.cast());
                    sz
                }
                LUA_VTHREAD => {
                    let sz = objsize(o);
                    self.traverse_thread(o.cast());
                    sz
                }
                t => {
                    debug_assert!(false, "propagatemark: unsupported kind {t:#x}");
                    0
                }
            };
            #[cfg(not(rustynail_lua54))]
            {
                (size / 16).max(1)
            }
            // 5.4 work units (`traversetable` = 1 + alimit + 2 * nodes, ...)
            #[cfg(rustynail_lua54)]
            {
                let _ = size;
                gen54::work54(o)
            }
        }
    }

    /// `getmode`: bit 0 = weak values, bit 1 = weak keys.
    unsafe fn getmode(&mut self, h: *mut Table) -> u8 {
        // SAFETY: G1.
        unsafe {
            let mt = if (*h).metatable.is_null() {
                None
            } else {
                Some(TableRef::from_raw((*h).metatable))
            };
            match self.fast_tm(mt, Tm::Mode) {
                Some(m) if m.is_string() => {
                    let b = m
                        .as_string()
                        .map(crate::string::StrView::as_bytes)
                        .unwrap_or(b"");
                    (b.contains(&b'v') as u8) | ((b.contains(&b'k') as u8) << 1)
                }
                _ => 0,
            }
        }
    }

    /// `iscleared`: strings are values (marked, never cleared); other objects are cleared iff white.
    unsafe fn iscleared(&mut self, o: *mut GcHeader) -> bool {
        // SAFETY: G1.
        unsafe {
            if o.is_null() {
                false
            } else if matches!((*o).tt, LUA_VSHRSTR | LUA_VLNGSTR) {
                self.markobject(o);
                false
            } else {
                iswhite(o)
            }
        }
    }

    /// `genlink`: keep a touched old object in `grayagain` so the next young collection revisits it.
    unsafe fn genlink(&mut self, o: *mut GcHeader) {
        // SAFETY: G1; `o` is black and has a `gclist`.
        unsafe {
            if getage(o) == G_TOUCHED1 {
                self.linkgclist(o, List::GrayAgain);
            } else if getage(o) == G_TOUCHED2 {
                setage(o, G_OLD); // advance age
            }
        }
    }

    unsafe fn traversetable(&mut self, h: *mut Table) {
        // SAFETY: G1; node/array access through `ga` follows the table invariants.
        unsafe {
            self.markobject_n((*h).metatable.cast());
            match self.getmode(h) {
                0 => self.traversestrong(h),
                1 => self.traverseweakvalue(h),
                2 => {
                    self.traverseephemeron(h, false);
                }
                _ => {
                    if self.gc.phase == GcPhase::Propagate {
                        self.linkgclist(h.cast(), List::GrayAgain);
                    } else {
                        self.linkgclist(h.cast(), List::AllWeak);
                    }
                }
            }
        }
    }

    unsafe fn traversearray(&mut self, h: *mut Table) -> bool {
        // SAFETY: G1.
        unsafe {
            let mut marked = false;
            for i in 0..ga::asize(h) as usize {
                let o = ga::arr_gc(h, i);
                if !o.is_null() && iswhite(o) {
                    marked = true;
                    self.reallymark(o);
                }
            }
            marked
        }
    }

    unsafe fn traversestrong(&mut self, h: *mut Table) {
        // SAFETY: G1.
        unsafe {
            self.traversearray(h);
            for i in 0..ga::nodes(h) {
                let n = ga::node(h, i);
                if ga::node_empty(n) {
                    ga::clearkey(n);
                } else {
                    let k = ga::node_key_gc(n);
                    if !k.is_null() {
                        self.markobject(k);
                    }
                    let v = ga::node_val_gc(n);
                    if !v.is_null() {
                        self.markobject(v);
                    }
                }
            }
            self.genlink(h.cast());
        }
    }

    unsafe fn traverseweakvalue(&mut self, h: *mut Table) {
        // SAFETY: G1.
        unsafe {
            let mut hasclears = ga::asize(h) > 0;
            for i in 0..ga::nodes(h) {
                let n = ga::node(h, i);
                if ga::node_empty(n) {
                    ga::clearkey(n);
                } else {
                    let k = ga::node_key_gc(n);
                    if !k.is_null() {
                        self.markobject(k);
                    }
                    if !hasclears && self.iscleared(ga::node_val_gc(n)) {
                        hasclears = true;
                    }
                }
            }
            if self.gc.phase == GcPhase::Propagate {
                self.linkgclist(h.cast(), List::GrayAgain);
            } else if hasclears {
                self.linkgclist(h.cast(), List::Weak);
            } else {
                self.genlink(h.cast());
            }
        }
    }

    /// Returns true iff some object was marked.
    unsafe fn traverseephemeron(&mut self, h: *mut Table, inv: bool) -> bool {
        // SAFETY: G1.
        unsafe {
            let mut hasclears = false;
            let mut hasww = false;
            let nsize = ga::nodes(h);
            let mut marked = self.traversearray(h);
            for i in 0..nsize {
                let n = if inv {
                    ga::node(h, nsize - 1 - i)
                } else {
                    ga::node(h, i)
                };
                if ga::node_empty(n) {
                    ga::clearkey(n);
                } else if self.iscleared(ga::node_key_gc(n)) {
                    hasclears = true;
                    let v = ga::node_val_gc(n);
                    if !v.is_null() && iswhite(v) {
                        hasww = true;
                    }
                } else {
                    let v = ga::node_val_gc(n);
                    if !v.is_null() && iswhite(v) {
                        marked = true;
                        self.reallymark(v);
                    }
                }
            }
            if self.gc.phase == GcPhase::Propagate {
                self.linkgclist(h.cast(), List::GrayAgain);
            } else if hasww {
                self.linkgclist(h.cast(), List::Ephemeron);
            } else if hasclears {
                self.linkgclist(h.cast(), List::AllWeak);
            } else {
                self.genlink(h.cast());
            }
            marked
        }
    }

    fn convergeephemerons(&mut self) {
        let mut dir = false;
        loop {
            let mut changed = false;
            let mut next = self.gc.ephemeron;
            self.gc.ephemeron = std::ptr::null_mut();
            while !next.is_null() {
                let w = next;
                // SAFETY: G1; list members are tables.
                unsafe {
                    let h = w.cast::<Table>();
                    next = (*h).gclist;
                    set2black(w);
                    if self.traverseephemeron(h, dir) {
                        self.propagateall();
                        changed = true;
                    }
                }
            }
            dir = !dir;
            if !changed {
                break;
            }
        }
    }

    // ---- clearing weak entries ----------------------------------------------------------------------

    fn clearbykeys(&mut self, mut l: *mut GcHeader) {
        while !l.is_null() {
            // SAFETY: G1; list members are tables.
            unsafe {
                let h = l.cast::<Table>();
                for i in 0..ga::nodes(h) {
                    let n = ga::node(h, i);
                    if self.iscleared(ga::node_key_gc(n)) {
                        ga::set_node_empty(n);
                    }
                    if ga::node_empty(n) {
                        ga::clearkey(n);
                    }
                }
                l = (*h).gclist;
            }
        }
    }

    fn clearbyvalues(&mut self, mut l: *mut GcHeader, f: *mut GcHeader) {
        while l != f {
            // SAFETY: G1; list members are tables; `f` is a suffix of the list.
            unsafe {
                let h = l.cast::<Table>();
                for i in 0..ga::asize(h) as usize {
                    let o = ga::arr_gc(h, i);
                    if self.iscleared(o) {
                        ga::set_arr_empty(h, i);
                    }
                }
                for i in 0..ga::nodes(h) {
                    let n = ga::node(h, i);
                    if self.iscleared(ga::node_val_gc(n)) {
                        ga::set_node_empty(n);
                    }
                    if ga::node_empty(n) {
                        ga::clearkey(n);
                    }
                }
                l = (*h).gclist;
            }
        }
    }

    // ---- atomic phase ---------------------------------------------------------------------------------

    fn atomic(&mut self, roots: &mut dyn FnMut(&mut Marker)) {
        let grayagain = std::mem::replace(&mut self.gc.grayagain, std::ptr::null_mut());
        debug_assert!(self.gc.ephemeron.is_null() && self.gc.weak.is_null());
        self.gc.phase = GcPhase::Atomic;
        roots(&mut Marker { heap: self });
        self.propagateall();
        self.remarkupvals();
        self.propagateall();
        self.gc.gray = grayagain;
        self.propagateall();
        self.convergeephemerons();
        // all strongly accessible objects are marked: clear values from weak tables
        self.clearbyvalues(self.gc.weak, std::ptr::null_mut());
        self.clearbyvalues(self.gc.allweak, std::ptr::null_mut());
        let origweak = self.gc.weak;
        let origall = self.gc.allweak;
        self.separatetobefnz(false);
        self.markbeingfnz();
        self.propagateall();
        self.convergeephemerons();
        // resurrected objects are marked: remove dead objects from weak tables
        self.clearbykeys(self.gc.ephemeron);
        self.clearbykeys(self.gc.allweak);
        self.clearbyvalues(self.gc.weak, origweak);
        self.clearbyvalues(self.gc.allweak, origall);
        // flip the current white
        self.current_white ^= WHITEBITS;
        debug_assert!(self.gc.gray.is_null());
    }

    // ---- sweep ------------------------------------------------------------------------------------------

    fn list_head(&mut self, l: ObjList) -> *mut *mut GcHeader {
        match l {
            ObjList::AllGc => &raw mut self.allgc,
            ObjList::FinObj => &raw mut self.gc.finobj,
            ObjList::ToBeFnz => &raw mut self.gc.tobefnz,
        }
    }

    /// `sweeplist`: sweep at most `max` objects of `list`, continuing at the cursor. Dead objects are
    /// freed, survivors repainted with the current white. Returns true when the list is exhausted.
    ///
    /// The predecessor is tracked as an object pointer (null = list head), never as a pointer into
    /// `self`, so `free_obj(&mut self)` calls cannot invalidate it.
    fn sweep_some(&mut self, list: ObjList, max: usize) -> bool {
        let ow = self.current_white ^ WHITEBITS;
        let white = self.current_white & WHITEBITS;
        let mut prev = self.gc.sweep_prev;
        let mut n = 0;
        // SAFETY: G1; every node is a live object until freed here.
        unsafe {
            loop {
                let curr = if prev.is_null() {
                    *self.list_head(list)
                } else {
                    (*prev).next
                };
                if curr.is_null() {
                    self.gc.sweep_prev = std::ptr::null_mut();
                    #[cfg(rustynail_lua54)]
                    {
                        self.gc.swept54 = n;
                    }
                    return true;
                }
                if n == max {
                    self.gc.sweep_prev = prev;
                    #[cfg(rustynail_lua54)]
                    {
                        self.gc.swept54 = n;
                    }
                    return false;
                }
                n += 1;
                let marked = (*curr).marked;
                // start fetching the next node's header while this one is processed (the list
                // walk is a chain of cache misses)
                #[cfg(target_arch = "x86_64")]
                std::arch::x86_64::_mm_prefetch::<{ std::arch::x86_64::_MM_HINT_T0 }>(
                    (*curr).next.cast::<i8>(),
                );
                if marked & ow != 0 {
                    let next = (*curr).next;
                    if prev.is_null() {
                        *self.list_head(list) = next;
                    } else {
                        (*prev).next = next;
                    }
                    self.free_obj(curr);
                } else {
                    (*curr).marked = (marked & !MASKGCBITS) | white | G_NEW;
                    prev = curr;
                }
            }
        }
    }

    /// `freeobj`.
    pub(crate) unsafe fn free_obj(&mut self, o: *mut GcHeader) {
        #[cfg(feature = "jit_trace")]
        if !self.gc.watch.is_empty() && self.gc.watch.contains(&(o as usize)) {
            self.gc.watch_hit = true;
        }
        // SAFETY: G1; `o` is unlinked.
        unsafe {
            match (*o).tt {
                LUA_VSHRSTR => {
                    self.remove_short(o.cast());
                    self.free_string(o.cast());
                }
                LUA_VLNGSTR => self.free_string(o.cast()),
                LUA_VTABLE => self.free_table(o.cast()),
                LUA_VLCL => {
                    let n = (*o.cast::<LClosure>()).nupvalues as usize;
                    self.mem.free(o.cast(), size_lclosure(n));
                }
                LUA_VCCL => {
                    let n = (*o.cast::<CClosure>()).nupvalues as usize;
                    self.mem.free(o.cast(), size_cclosure(n));
                }
                LUA_VUPVAL => self.free_upval(o.cast()),
                LUA_VPROTO => self.free_proto(o.cast()),
                LUA_VUSERDATA => {
                    let u = o.cast::<Udata>();
                    let sz = size_udata((*u).nuvalue as usize, (*u).len).unwrap_or(0);
                    self.mem.free(o.cast(), sz);
                }
                LUA_VTHREAD => crate::state::free_thread(self, o.cast()),
                t => debug_assert!(false, "free_obj: unsupported kind {t:#x} (leaked)"),
            }
        }
    }

    /// `checkSizes`: shrink the string table if it is too big. B3-1: hysteresis, shrink only when fewer than
    /// size/16 strings are live (the reference uses size/4, which makes the table oscillate: grow at nuse >= size,
    /// shrink after a minor collection at nuse < size/4, regrow). Memory only; no observable behaviour
    /// (the `next`/pairs order of strings does not depend on the string-table size).
    fn check_sizes(&mut self) {
        if !self.gc.emergency && self.strtab_len() < self.strtab_size() / 16 {
            #[cfg(rustynail_lua54)]
            let before = self.mem.total_bytes();
            self.resize_strtab(self.strtab_size() / 2);
            #[cfg(rustynail_lua54)]
            {
                let freed = before.saturating_sub(self.mem.total_bytes());
                self.gc.estimate = self.gc.estimate.saturating_sub(freed);
            }
        }
    }

    // ---- finalizers -----------------------------------------------------------------------------------------

    /// `luaT_gettm` with the flag cache: the fast metamethod `e` of `mt`, if present.
    pub fn fast_tm(&self, mt: Option<TableRef>, e: Tm) -> Option<Value> {
        let mt = mt?;
        if mt.flags() & (1u8 << e as u8) != 0 {
            return None;
        }
        debug_assert!((e as u8) <= Tm::Eq as u8);
        // SAFETY: tm_names are live fixed short strings.
        let name = unsafe { StrRef(std::ptr::NonNull::new_unchecked(self.tm_names[e as usize])) };
        let v = mt.get_short_str(name);
        if v.is_nil() {
            // SAFETY: handle invariant; flags are plain data.
            unsafe { (*mt.as_ptr()).flags |= 1u8 << e as u8 };
            None
        } else {
            Some(v)
        }
    }

    /// `luaC_checkfinalizer`: if `o` (a table; B3: userdata) has a metatable with `__gc`, move it to
    /// the `finobj` list.
    ///
    /// # Safety
    /// `o` must be a live object of this heap whose metatable is `mt`.
    pub unsafe fn check_finalizer(&mut self, o: *mut GcHeader, mt: Option<TableRef>) {
        // SAFETY: G1.
        unsafe {
            if tofinalize(o)
                || self.fast_tm(mt, Tm::Gc).is_none()
                || self.gc.stp & GCSTP_CLOSING != 0
            {
                return;
            }
            if self.gc.phase < GcPhase::SweepAllGc {
                // correct the generational boundary pointers that point to `o`
                for q in [
                    &raw mut self.gc.survival,
                    &raw mut self.gc.old1,
                    &raw mut self.gc.reallyold,
                    &raw mut self.gc.firstold1,
                ] {
                    if *q == o {
                        *q = (*o).next;
                    }
                }
            }
            let mut p: *mut *mut GcHeader = &raw mut self.allgc;
            let mut pred: *mut GcHeader = std::ptr::null_mut();
            while *p != o {
                pred = *p;
                p = &raw mut (**p).next;
            }
            *p = (*o).next;
            (*o).next = self.gc.finobj;
            self.gc.finobj = o;
            (*o).marked |= FINALIZEDBIT;
            if self.gc.phase >= GcPhase::SweepAllGc {
                // sweep phase: the object must not stay black on the (maybe already swept) list
                // and the sweep cursor must not point at it
                self.makewhite(o);
                if self.gc.phase == GcPhase::SweepAllGc && self.gc.sweep_prev == o {
                    self.gc.sweep_prev = pred;
                }
            }
        }
    }

    /// `separatetobefnz`: move unreachable (or all) finalizable objects to `tobefnz`.
    fn separatetobefnz(&mut self, all: bool) {
        // SAFETY: G1/G3.
        unsafe {
            let mut p: *mut *mut GcHeader = &raw mut self.gc.finobj;
            let mut lastnext: *mut *mut GcHeader = &raw mut self.gc.tobefnz;
            while !(*lastnext).is_null() {
                lastnext = &raw mut (**lastnext).next;
            }
            // objects after `finobjold1` cannot be white (incremental mode: it is null)
            while !(*p).is_null() && *p != self.gc.finobjold1 {
                let curr = *p;
                debug_assert!(tofinalize(curr));
                if !(iswhite(curr) || all) {
                    p = &raw mut (*curr).next;
                } else {
                    if curr == self.gc.finobjsur {
                        self.gc.finobjsur = (*curr).next; // removing `finobjsur`: correct it
                    }
                    *p = (*curr).next;
                    (*curr).next = *lastnext;
                    *lastnext = curr;
                    lastnext = &raw mut (*curr).next;
                }
            }
        }
    }

    /// `udata2finalize`: take the next object from `tobefnz` back to `allgc`.
    pub(crate) fn udata2finalize(&mut self) -> *mut GcHeader {
        let o = self.gc.tobefnz;
        // SAFETY: G1/G3.
        unsafe {
            debug_assert!(tofinalize(o));
            self.gc.tobefnz = (*o).next;
            (*o).next = self.allgc;
            self.allgc = o;
            (*o).marked &= !FINALIZEDBIT;
            if self.gc.phase >= GcPhase::SweepAllGc {
                self.makewhite(o); // sweep phase: do not leave a black object behind the cursor
            } else if getage(o) == G_OLD1 {
                self.gc.firstold1 = o; // it is the first OLD1 object in the list
            }
        }
        o
    }

    /// `callallpendingfinalizers`: run the finalizer hook for every pending object. Collections are
    /// suppressed while hooks run.
    pub fn call_all_pending_finalizers(&mut self) {
        if self.gc.fin_hook.is_none() {
            return;
        }
        let mut hook = self.gc.fin_hook.take();
        let oldstp = self.gc.stp;
        self.gc.stp |= GCSTP_GC;
        while !self.gc.tobefnz.is_null() {
            let o = self.udata2finalize();
            if let Some(h) = hook.as_mut() {
                h(self, o);
            }
        }
        self.gc.stp = oldstp;
        if self.gc.fin_hook.is_none() {
            self.gc.fin_hook = hook;
        }
    }

    /// `luaC_freeallobjects` (finalizer part): mark everything finalizable for finalization and run the
    /// finalizers. Call before dropping the heap if `__gc` must run at close. Objects are freed by
    /// `Drop`.
    pub fn close(&mut self) {
        self.gc.stp |= GCSTP_CLOSING;
        self.leave_generational();
        self.separatetobefnz(true);
        self.call_all_pending_finalizers();
        // objects created by finalizers may have gotten finalizers too (not registered: CLOSING)
    }

    /// Free every object (used by `Drop`).
    pub(crate) fn free_all_objects(&mut self) {
        for list in [
            std::mem::replace(&mut self.allgc, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.finobj, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.tobefnz, std::ptr::null_mut()),
            std::mem::replace(&mut self.gc.fixedgc, std::ptr::null_mut()),
        ] {
            let mut p = list;
            while !p.is_null() {
                // SAFETY: G1 — each node is live until freed here; `next` is read first. Short
                // strings unlink themselves from the string table, which stays consistent.
                unsafe {
                    let next = (*p).next;
                    self.free_obj(p);
                    p = next;
                }
            }
        }
    }

    // ---- verification (tests / debugging) --------------------------------------------------------------------

    /// Does the strong tri-colour invariant (no black object points to a white one) hold now?
    /// (Not in generational minor mode: old1/touched2 objects may point to white survivals.)
    fn strong_invariant(&self) -> bool {
        self.keepinvariant() && self.gc.kind != GcKind::GenMinor
    }

    /// Call `f` for every collectable object directly referenced by `o` (not for threads' stacks
    /// beyond `top`; open upvalues reference their stack slot's value).
    ///
    /// # Safety
    /// `o` must be a live object of this heap.
    unsafe fn for_each_child(&self, o: *mut GcHeader, f: &mut dyn FnMut(*mut GcHeader)) {
        fn emit_v(f: &mut dyn FnMut(*mut GcHeader), v: &Value) {
            if let Some(g) = v.as_gc() {
                f(g)
            }
        }
        fn emit_p(f: &mut dyn FnMut(*mut GcHeader), p: *mut GcHeader) {
            if !p.is_null() {
                f(p)
            }
        }
        // SAFETY: G1 and the per-kind layout invariants used by `verify_refs`.
        unsafe {
            match (*o).tt {
                LUA_VTABLE => {
                    let h = o.cast::<Table>();
                    if !(*h).metatable.is_null() {
                        emit_p(f, (*h).metatable.cast());
                    }
                    for i in 0..ga::asize(h) as usize {
                        let g = ga::arr_gc(h, i);
                        if !g.is_null() {
                            emit_p(f, g);
                        }
                    }
                    for i in 0..ga::nodes(h) {
                        let n = ga::node(h, i);
                        if !ga::node_empty(n) {
                            let k = ga::node_key_gc(n);
                            if !k.is_null() {
                                emit_p(f, k);
                            }
                            let v = ga::node_val_gc(n);
                            if !v.is_null() {
                                emit_p(f, v);
                            }
                        }
                    }
                }
                LUA_VLCL => {
                    let c = Gc::from_raw(o.cast::<LClosure>());
                    emit_p(f, (*c.as_ptr()).p.cast());
                    for i in 0..c.nupvalues() {
                        emit_p(f, (*c.upvals_ptr().add(i)).cast());
                    }
                }
                LUA_VCCL => {
                    let c = Gc::from_raw(o.cast::<CClosure>());
                    for i in 0..c.nupvalues() {
                        emit_v(f, &c.upvalue(i));
                    }
                }
                LUA_VUPVAL => {
                    let uv = Gc::from_raw(o.cast::<UpVal>());
                    if !uv.is_open() {
                        emit_v(f, &uv.get());
                    }
                }
                LUA_VUSERDATA => {
                    let u = Gc::from_raw(o.cast::<Udata>());
                    emit_p(f, (*u.as_ptr()).metatable.cast());
                    for i in 0..u.nuvalue() {
                        emit_v(f, &u.user_value(i).unwrap());
                    }
                }
                LUA_VPROTO => {
                    let p = Gc::from_raw(o.cast::<Proto>());
                    emit_p(f, p.source.cast());
                    for v in p.k() {
                        emit_v(f, v);
                    }
                    for i in 0..p.p.cap() {
                        emit_p(f, (*p.p.as_ptr().add(i)).cast());
                    }
                }
                _ => {}
            }
        }
    }

    /// Generational invariants between collections (tests): young objects are white; an `OLD`
    /// object points only to old objects; `OLD1`/`TOUCHED2` objects do not point to new ones.
    fn verify_gen(&self, objs: &[*mut GcHeader]) {
        let other_white = self.current_white ^ WHITEBITS;
        for &o in objs {
            // SAFETY: listed object.
            unsafe {
                let m = (*o).marked;
                if m & other_white != 0 || (*o).tt == LUA_VTHREAD {
                    continue;
                }
                let age = getage(o);
                if age <= G_SURVIVAL {
                    assert!(iswhite(o), "young object (age {age}) is not white");
                    continue;
                }
                if (*o).tt == LUA_VUPVAL && Gc::from_raw(o.cast::<UpVal>()).is_open() {
                    continue;
                }
                let mut bad = None;
                self.for_each_child(o, &mut |c| {
                    let ca = getage(c);
                    let need_old = age == G_OLD;
                    let need_nonnew = matches!(age, G_OLD1 | G_TOUCHED2);
                    if (need_old && ca <= G_SURVIVAL) || (need_nonnew && ca == G_NEW) {
                        bad = Some((age, ca, (*c).tt));
                    }
                });
                assert!(
                    bad.is_none(),
                    "old object points to a younger one: (parent age, child age, child tt) = {bad:?}"
                );
            }
        }
    }

    /// Check heap invariants outside a collection; panics with a message on violation.
    pub fn verify(&self) {
        use std::collections::HashSet;
        let mut all: HashSet<usize> = HashSet::new();
        let mut objs: Vec<*mut GcHeader> = Vec::new();
        let mut shorts = 0usize;
        let lists = [self.allgc, self.gc.finobj, self.gc.tobefnz, self.gc.fixedgc];
        let other_white = self.current_white ^ WHITEBITS;
        for (li, l) in lists.iter().enumerate() {
            let mut p = *l;
            while !p.is_null() {
                // SAFETY: G1.
                unsafe {
                    assert!(
                        all.insert(p as usize),
                        "object on two lists / cycle in list"
                    );
                    objs.push(p);
                    let m = (*p).marked;
                    if li != 3 && self.gc.phase == GcPhase::Pause {
                        assert_eq!(m & other_white, 0, "dead-white object outside collection");
                        assert_eq!(m & BLACKBIT, 0, "black object outside collection");
                    }
                    assert_eq!(
                        tofinalize(p),
                        li == 1 || li == 2,
                        "finalized bit mismatch on list {li}"
                    );
                    match (*p).tt {
                        LUA_VSHRSTR => {
                            shorts += 1;
                            let s = &*p.cast::<LuaString>();
                            assert!(s.is_short() && s.len() <= crate::string::MAX_SHORT_LEN);
                        }
                        LUA_VLNGSTR => {}
                        LUA_VTABLE | LUA_VLCL | LUA_VCCL | LUA_VUPVAL | LUA_VPROTO
                        | LUA_VUSERDATA | LUA_VTHREAD => {}
                        t => panic!("unknown object tag {t:#x}"),
                    }
                    p = (*p).next;
                }
            }
        }
        self.verify_refs(&all, &objs);
        if self.gc.kind == GcKind::GenMinor && self.gc.phase == GcPhase::Propagate {
            self.verify_gen(&objs);
        }
        assert_eq!(shorts, self.strtab_len(), "string table count mismatch");
        // string table: every entry is on a list, in the right bucket
        let mut intable = 0usize;
        // SAFETY: H2.
        unsafe {
            for b in 0..self.strtab_size() {
                let mut s = self.strtab_bucket(b);
                while !s.is_null() {
                    assert!(
                        all.contains(&(s as usize)),
                        "string in table but not on a list"
                    );
                    assert_eq!(
                        LuaString::strtab_hash(s, self.seed()) as usize & (self.strtab_size() - 1),
                        b,
                        "string in wrong bucket"
                    );
                    // I-T: the tag in the link word is the low bits of the string-table hash
                    if cfg!(all(target_arch = "x86_64", target_pointer_width = "64"))
                        && (*s).extra_raw() != 0
                    {
                        assert_eq!(
                            LuaString::strtab_tag(s),
                            LuaString::strtab_hash(s, self.seed()) as usize & ((1usize << 19) - 1),
                            "wrong strtab tag"
                        );
                    }
                    intable += 1;
                    s = LuaString::hnext(s);
                }
            }
        }
        assert_eq!(intable, shorts, "table/list short string mismatch");
        // table contents point to live objects
        for &o in &objs {
            // SAFETY: G1.
            unsafe {
                // objects condemned by the current cycle may point to already freed objects
                if (*o).marked & other_white != 0 {
                    continue;
                }
                if (*o).tt == LUA_VTABLE {
                    let h = o.cast::<Table>();
                    let blk = self.strong_invariant() && (*o).marked & BLACKBIT != 0;
                    let nowhite = |g: *mut GcHeader| {
                        assert!(
                            !blk || g.is_null() || !iswhite(g),
                            "black table points to a white object"
                        )
                    };
                    nowhite((*h).metatable.cast());
                    for i in 0..ga::asize(h) as usize {
                        nowhite(ga::arr_gc(h, i));
                    }
                    for i in 0..ga::nodes(h) {
                        let n = ga::node(h, i);
                        if !ga::node_empty(n) {
                            nowhite(ga::node_key_gc(n));
                            nowhite(ga::node_val_gc(n));
                        }
                    }
                    if !(*h).metatable.is_null() {
                        assert!(
                            all.contains(&((*h).metatable as usize)),
                            "dangling metatable"
                        );
                    }
                    for i in 0..ga::asize(h) as usize {
                        let g = ga::arr_gc(h, i);
                        assert!(
                            g.is_null() || all.contains(&(g as usize)),
                            "dangling array value"
                        );
                    }
                    for i in 0..ga::nodes(h) {
                        let n = ga::node(h, i);
                        let k = ga::node_key_gc(n);
                        let v = ga::node_val_gc(n);
                        assert!(k.is_null() || all.contains(&(k as usize)), "dangling key");
                        if !ga::node_empty(n) {
                            assert!(v.is_null() || all.contains(&(v as usize)), "dangling value");
                        }
                    }
                }
            }
        }
    }

    /// Check that references held by closures, upvalues, userdata, protos and threads point to
    /// live objects (tests; complements [`Heap::verify`]).
    fn verify_refs(&self, all: &std::collections::HashSet<usize>, objs: &[*mut GcHeader]) {
        let parent_black = std::cell::Cell::new(false);
        let chk = |v: &Value, what: &str| {
            if let Some(g) = v.as_gc() {
                assert!(all.contains(&(g as usize)), "dangling {what}");
                // SAFETY: `g` was just checked to be a listed (live) object.
                let white = unsafe { iswhite(g) };
                assert!(
                    !parent_black.get() || !white,
                    "black object points to white {what}"
                );
            }
        };
        let chkp = |p: *mut GcHeader, what: &str| {
            assert!(
                p.is_null() || all.contains(&(p as usize)),
                "dangling {what}"
            );
            // SAFETY: `p` is null or was just checked to be a listed (live) object.
            let white = !p.is_null() && unsafe { iswhite(p) };
            assert!(
                !parent_black.get() || !white,
                "black object points to white {what}"
            );
        };
        let other_white = self.current_white ^ WHITEBITS;
        for &o in objs {
            // SAFETY: listed object.
            if unsafe { (*o).marked & other_white != 0 } {
                continue; // dead, not yet swept
            }
            // SAFETY: listed object.
            parent_black.set(unsafe {
                self.strong_invariant() && (*o).marked & BLACKBIT != 0 && (*o).tt != LUA_VTHREAD
            });
            // SAFETY: G1 — all listed objects are live; pointer slots follow F1/F4/U1/ST1-ST3.
            unsafe {
                match (*o).tt {
                    LUA_VLCL => {
                        let c = Gc::from_raw(o.cast::<LClosure>());
                        chkp((*c.as_ptr()).p.cast(), "closure proto");
                        for i in 0..c.nupvalues() {
                            chkp((*c.upvals_ptr().add(i)).cast(), "closure upvalue");
                        }
                    }
                    LUA_VCCL => {
                        let c = Gc::from_raw(o.cast::<CClosure>());
                        for i in 0..c.nupvalues() {
                            chk(&c.upvalue(i), "C closure upvalue");
                        }
                    }
                    LUA_VUPVAL => {
                        let uv = Gc::from_raw(o.cast::<UpVal>());
                        if !uv.is_open() {
                            chk(&uv.get(), "closed upvalue value");
                        }
                    }
                    LUA_VUSERDATA => {
                        let u = Gc::from_raw(o.cast::<Udata>());
                        chkp((*u.as_ptr()).metatable.cast(), "userdata metatable");
                        for i in 0..u.nuvalue() {
                            chk(&u.user_value(i).unwrap(), "user value");
                        }
                    }
                    LUA_VPROTO => {
                        let p = Gc::from_raw(o.cast::<Proto>());
                        chkp(p.source.cast(), "proto source");
                        for v in p.k() {
                            chk(v, "proto constant");
                        }
                        for i in 0..p.p.cap() {
                            chkp((*p.p.as_ptr().add(i)).cast(), "nested proto");
                        }
                    }
                    LUA_VTHREAD => {
                        let th = o.cast::<Thread>();
                        if (*th).stack.cap() != 0 {
                            for i in 0..(*th).top.0 {
                                chk(&*(*th).stack.as_ptr().add(i), "stack value");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Number of objects on all lists (tests).
    pub fn object_count(&self) -> usize {
        let mut n = 0;
        for l in [self.allgc, self.gc.finobj, self.gc.tobefnz, self.gc.fixedgc] {
            let mut p = l;
            while !p.is_null() {
                n += 1;
                // SAFETY: G1.
                p = unsafe { (*p).next };
            }
        }
        n
    }
}
