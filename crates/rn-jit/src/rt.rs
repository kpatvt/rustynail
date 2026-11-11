//! Runtime: configuration (`RUSTYNAIL_JIT`, `RUSTYNAIL_JIT_THRESHOLD`), per-state code arena, compile
//! driver, the `jit_enter` wrapper and registration with the interpreter's hooks
//! (`rn_core::jit_hooks`). Everything `unsafe` here is about calling generated code and about the
//! `Proto.jit` descriptor pointer (see docs/UNSAFE.md, rn-jit section).

use crate::lower::calls::{CallEnv, JitProtoHdr};
use crate::lower::{LowerCfg, Lowered};
use crate::mem::{page_size, probe_exec, CodeArena, CodeBlock, PerfMap};
use crate::t15::runtime::{
    self as t15rt, NestPlan, T15Config, T15Data, T15Mode, T15Proto, VersionData,
};
use crate::x86::{self, ThunkEx};
use rn_core::func::Proto;
use rn_core::jit_helpers;
use rn_core::jit_hooks::{self, HotCall, JitExit, VTable};
use rn_core::state::{Ci, State, StkId};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::OnceLock;

/// `RUSTYNAIL_JIT` mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No compilation.
    Off,
    /// Compile hot functions and loops (default thresholds 200 calls / 1000 back-edges).
    On,
    /// Compile every function on its first call (loops on their first back-edge).
    Always,
    /// `Always` plus a quarantined arena (freed code faults deterministically).
    Verify,
}

/// Process-wide configuration, read once by [`register`].
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Mode of states created from now on.
    pub mode: Mode,
    /// Calls before a function is compiled.
    pub entry_calls: i32,
    /// Back-edges per loop sampling tick.
    pub loop_edges: i32,
}

impl Config {
    /// The configuration for `mode` with its default thresholds.
    pub fn for_mode(mode: Mode) -> Config {
        match mode {
            Mode::Always | Mode::Verify => Config {
                mode,
                entry_calls: 1,
                loop_edges: 1,
            },
            _ => Config {
                mode,
                entry_calls: jit_hooks::DEFAULT_ENTRY_CALLS,
                loop_edges: jit_hooks::TICK_PERIOD,
            },
        }
    }

    /// Reads `RUSTYNAIL_JIT=off|on|always|verify` (default `on`) and `RUSTYNAIL_JIT_THRESHOLD=entry,loop`.
    pub fn from_env() -> Config {
        let mode = std::env::var("RUSTYNAIL_JIT").ok();
        let thr = std::env::var("RUSTYNAIL_JIT_THRESHOLD").ok();
        Config::parse(mode.as_deref(), thr.as_deref())
    }

    /// The pure part of [`Config::from_env`]: `mode` is the value of `RUSTYNAIL_JIT`, `thr` the value
    /// of `RUSTYNAIL_JIT_THRESHOLD` (`entry,loop`; unparsable parts keep the mode's default).
    pub fn parse(mode: Option<&str>, thr: Option<&str>) -> Config {
        let lower = mode.map(|m| m.trim().to_ascii_lowercase());
        let mode = match lower.as_deref() {
            Some("off" | "0" | "false") => Mode::Off,
            Some("always") => Mode::Always,
            Some("verify") => Mode::Verify,
            _ => Mode::On,
        };
        let mut c = Config::for_mode(mode);
        if let Some(t) = thr {
            let mut it = t.split(',').map(|x| x.trim().parse::<i32>().ok());
            if let Some(Some(e)) = it.next() {
                c.entry_calls = e.max(1);
            }
            if let Some(Some(l)) = it.next() {
                c.loop_edges = l.max(1);
            }
        }
        c
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Registers the compiler with the interpreter, configured from the environment. Idempotent; call it
/// before creating states (thresholds are baked into new prototypes and threads).
pub fn register() {
    register_with(Config::from_env());
}

/// Test harness entry (F9): [`register_with`] after pinning `RUSTYNAIL_JIT_TRACE=off` for the process unless the
/// environment chose a value. Traces default to `on` now; the J4 / tier 1.5 tests and the `tr_*` tests that switch
/// traces on per state (`set_trace` + `attach_driver`) mean "no traces unless I ask", so they register through this.
/// Must be the first thing a test binary does (the process configuration is read once).
#[doc(hidden)]
pub fn register_with_trace_pinned_off(cfg: Config) -> bool {
    if std::env::var_os("RUSTYNAIL_JIT_TRACE").is_none() {
        std::env::set_var("RUSTYNAIL_JIT_TRACE", "off");
    }
    register_with(cfg)
}

/// [`register`] with an explicit configuration; the first registration of the process wins. Returns
/// whether this call installed the compiler. `Mode::Off` installs nothing.
pub fn register_with(cfg: Config) -> bool {
    let first = CONFIG.set(cfg).is_ok();
    rn_std::lib_ext::set_jit_ctl(rn_std::lib_ext::JitCtl {
        status: ctl_status,
        set_enabled: ctl_set_enabled,
        flush: ctl_flush,
    });
    if first && cfg.mode != Mode::Off {
        jit_hooks::set_thresholds(cfg.entry_calls, cfg.loop_edges);
        jit_hooks::set_stress_stackmove(
            std::env::var("RUSTYNAIL_JIT_STRESS")
                .is_ok_and(|v| v.split(',').any(|x| x == "stackmove")),
        );
        let (next, ipairs_aux) = rn_std::lib_base::jit_builtins();
        jit_hooks::set_builtins(next, ipairs_aux);
        jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
        {
            // P-B: the library functions the J5 recorder runs as intrinsics (order of `jit_hooks::libfn`)
            jit_hooks::set_strlen(rn_std::lib_string::jit_len_addr());
            jit_hooks::set_strformat(rn_std::lib_string::jit_format_addr());
            jit_hooks::set_select(rn_std::lib_base::jit_select_addr());
            let [sub, byte, chr] = rn_std::lib_string::jit_libfns();
            let [ins, rem] = rn_std::lib_table::jit_libfns();
            let [mty, fmod, toint] = rn_std::lib_math::jit_libfns();
            let ([tostr, rawlen, rawequal, rawget, rawset, tonum], typefn) =
                rn_std::lib_base::jit_libfns();
            jit_hooks::set_libfns(
                [
                    sub, byte, chr, mty, tostr, rawlen, rawequal, rawget, rawset, ins, rem, fmod,
                    toint, tonum,
                ],
                typefn,
            );
            jit_hooks::set_libfns_ext(rn_std::lib_table::jit_libfns_ext());
        }
        // J5: with traces on in the environment, the pipeline driver is the process's recorder (J5 PI)
        #[cfg(feature = "trace")]
        if crate::tr::cache::TrConfig::process().checked().mode != crate::tr::cache::TrMode::Off {
            crate::tr::install_driver();
        }
        jit_hooks::install(VTable {
            hot_call,
            hot_loop,
            enter,
            free_state,
            free_code,
        })
    } else {
        false
    }
}

/// Counters of one state (for tests and `RUSTYNAIL_JIT_VERBOSE`-style reporting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Prototypes compiled.
    pub compiled: u64,
    /// Prototypes that could not be compiled (marked do-not-compile).
    pub failed: u64,
    /// Entries into compiled code.
    pub enters: u64,
    /// ... of which at a pc other than 0 (OSR, resume points).
    pub osr_enters: u64,
    /// Exits `Continue`.
    pub exit_continue: u64,
    /// Exits `FrameChanged`.
    pub exit_frame_changed: u64,
    /// Exits `Returned`.
    pub exit_returned: u64,
    /// Bytecodes executed by compiled code, summed over all entries (the block-count budget consumed; a
    /// nested entry is counted for itself, not for the activation that called it).
    pub insns: u64,
    /// Bytes of machine code generated.
    pub code_bytes: u64,
    /// Compiled prototypes whose code was freed (the prototype was collected).
    pub freed: u64,
    /// Compilations refused for a transient reason (code cap / memory limit); retried later.
    pub refused: u64,
    /// Prototypes that currently own compiled code (a snapshot taken by [`stats`]).
    pub live: u64,
    /// Address space mapped by the code arena (snapshot).
    pub mapped: u64,
    /// Bytes of sealed (executable) pages in the arena's live chunks (snapshot).
    pub sealed: u64,
    /// Tier 1.5: nests promoted (the prototype was recompiled for them).
    pub t15_promoted: u64,
    /// Tier 1.5: recompiles (promotions and respecialisations) that produced a new code version.
    pub t15_recompiled: u64,
    /// Tier 1.5: promotions refused for a transient reason (code cap / memory limit; retried with back-off).
    pub t15_refused: u64,
    /// Tier 1.5: nests that went `Off` (permanent failure, compile cap, back-off exhausted).
    pub t15_off: u64,
    /// Tier 1.5: typed regions emitted into recompiled code (a recompile re-emits the regions of earlier promotions,
    /// so this counts emissions, not distinct nests).
    pub t15_regions: u64,
    /// Tier 1.5 policy: failure-countdown events (a nest's stubs failed `EXIT_HOT` times).
    pub t15_stub_events: u64,
    /// Tier 1.5 policy: respecialisations (recompiles with a retargeted or widened speculation).
    pub t15_respec: u64,
    /// Tier 1.5 policy: regions given up (back to tier 1).
    pub t15_giveups: u64,
    /// Tier 1.5 policy: decisions that found the failure rate benign (thrash penalty: the next decision waits for
    /// twice as many failures).
    pub t15_penalties: u64,
    /// Tier 1.5 policy: failure countdowns that fired in the pads of a retired (stale) code version, i.e. activations
    /// that really ran in retired blocks.
    pub t15_stale_events: u64,
    /// Tier 1.5 policy: intrinsic call sites demoted to ordinary calls.
    pub t15_demotions: u64,
}

/// Descriptor behind `Proto.jit` (`Box::into_raw`; owned by the prototype, freed with it through
/// [`free_code`], or by the state's [`JitData`] at close). `hdr` must stay first: compiled code reads it.
#[repr(C)]
pub(crate) struct JitProto {
    pub(crate) hdr: JitProtoHdr,
    /// Worth entering from the interpreter (`Lowered::interp_entry`).
    pub(crate) interp_entry: bool,
    /// Native offset + 1 (0 = no entry) per instruction; `hdr.resume` points at its buffer. Indexed by the
    /// *byte* offset of the instruction by the generated return sequence.
    pub(crate) resume: Vec<u32>,
    pub(crate) code_ptr: *const u32,
    /// The current code block (`hdr.base_m1 + 1`). Older versions are in `t15.retired`.
    pub(crate) block: CodeBlock,
    pub(crate) owner: *mut JitData,
    /// Bytes charged to the memory limit for this proto (refunded when it is freed).
    pub(crate) charged: usize,
    /// Short-stay monitor of the interpreter entries (back-off, see [`Mon`]). Shared (`Rc`) so that `enter`
    /// can keep it across a call of compiled code that may free this descriptor (the prototype can be
    /// collected while its own stay is still running, see [`enter`]).
    mon: Rc<Mon>,
    /// Set (debug builds) when the owning prototype was collected: the descriptor is then kept in
    /// `JitData::quarantine` instead of being freed, and [`JitProto::mon`] asserts it is not used any more.
    dead: std::cell::Cell<bool>,
    /// Tier 1.5 state (warm-up counters, retired code versions); `None` when the prototype has no loop nest or
    /// tier 1.5 is off. Lives and dies with the descriptor, like every code block it owns.
    pub(crate) t15: Option<Box<T15Proto>>,
    /// J5 trace table (dispatch cells, counters, penalties); `None` when the prototype has no trace candidate or
    /// traces are off (docs/JIT_TRACE.md 3.1).
    #[cfg(feature = "trace")]
    pub(crate) tr: Option<Box<crate::tr::cache::TrProto>>,
}

impl JitProto {
    /// N3b: see [`Mon::reset`].
    pub(crate) fn reset_entry_backoff(&self) {
        self.mon.reset();
    }

    #[inline]
    fn mon(&self) -> &Mon {
        debug_assert!(!self.dead.get(), "use of a freed JitProto descriptor");
        &self.mon
    }
}

/// Entry profitability monitor of one compiled prototype. Entering compiled code from the interpreter
/// (function entry or OSR) and leaving it again costs a fixed ~150 cycles; a compiled stay that runs only a
/// few instructions before it exits (an exit stub early in the function, a metamethod frame, a tail call that
/// the interpreter finishes) never pays that back. Every [`MON_WINDOW`] entries the average number of
/// bytecodes executed per stay (the block-count budget consumed) is checked; below [`MIN_STAY`] the
/// prototype is *blocked* for interpreter entries for `skip` entry opportunities, doubling at every failed
/// retrial (LuaJIT-style back-off, docs/JIT_PLAN.md section 14 item 2). Compiled callers keep calling it
/// directly (`hdr.entry0`); only the interpreter's entries are affected.
#[derive(Default)]
struct Mon {
    enters: std::cell::Cell<u32>,
    insns: std::cell::Cell<u64>,
    /// Remaining entry opportunities to refuse (0 = not blocked).
    skip: std::cell::Cell<u32>,
    /// Next block length.
    next_skip: std::cell::Cell<u32>,
    /// Times blocked / entries refused (statistics).
    blocks: std::cell::Cell<u32>,
}

/// Entries per monitor window.
const MON_WINDOW: u32 = 64;
/// Minimum average bytecodes per stay in compiled code for entries from the interpreter to be worth it.
const MIN_STAY: u64 = 48;
/// First block length, in entry opportunities. A `hot_loop` firing (a loop sampling tick) is one opportunity;
/// a `hot_call` firing (each stands for `DEFAULT_ENTRY_CALLS` calls) weighs that many, so the first call block
/// lasts one firing (measured: weighing calls 1 too blocks metamethod-heavy code for too long). The loop tick
/// used to weigh `TICK_PERIOD`, which made the first loop block last a single tick and never grow in practice.
const FIRST_SKIP: u32 = 64;
/// N3b: a stay of at least this many bytecodes ran a loop (a trace) for a while.
const LONG_STAY: u64 = 256;
const MAX_SKIP: u32 = 1 << 20;

impl Mon {
    /// Records a finished stay of `n` bytecodes.
    fn stay(&self, n: u64) {
        let e = self.enters.get() + 1;
        let t = self.insns.get() + n;
        if e < MON_WINDOW {
            self.enters.set(e);
            self.insns.set(t);
            return;
        }
        self.enters.set(0);
        self.insns.set(0);
        if t < u64::from(MON_WINDOW) * MIN_STAY {
            let k = self.next_skip.get().max(FIRST_SKIP);
            self.skip.set(k);
            self.next_skip.set((k * 2).min(MAX_SKIP));
            self.blocks.set(self.blocks.get() + 1);
        } else {
            self.next_skip.set(0);
        }
    }

    /// N3b: forgets the back-off: the compiled code the entries were judged on changed (a trace was installed in the prototype).
    pub(crate) fn reset(&self) {
        self.skip.set(0);
        self.next_skip.set(0);
        self.enters.set(0);
        self.insns.set(0);
    }

    /// An entry opportunity of weight `w`: true when the entry is refused.
    fn blocked(&self, w: u32) -> bool {
        let s = self.skip.get();
        if s == 0 {
            return false;
        }
        self.skip.set(s.saturating_sub(w.max(1)));
        true
    }
}

/// Value of `Proto.jit` meaning "do not compile".
pub(crate) const NOJIT: usize = 1;

/// What `r15` points at: the block-count budget word, and (feature `trace`) the J5 exit area right behind it at the
/// frozen distance `tr::exit_area::FROM_R15`. Generated code only ever addresses `r15` + small offsets, never a
/// `JitData` field, so this prefix adds no code difference; without `trace` it is the bare word.
#[repr(C)]
pub(crate) struct BudgetHdr {
    pub(crate) cnt: i32,
    _pad: u32,
    #[cfg(feature = "trace")]
    pub(crate) area: crate::tr::ExitArea,
    /// P2-3c: the pool of double constants (traces address it as `[r15 + pool_off]`).
    #[cfg(feature = "trace")]
    pub(crate) fpool: crate::tr::FPool,
}

#[cfg(feature = "trace")]
const _: () = {
    assert!(std::mem::offset_of!(BudgetHdr, area) == crate::tr::exit_area::FROM_R15 as usize);
    assert!(std::mem::offset_of!(JitData, hdr) == 0);
    assert!(
        std::mem::offset_of!(BudgetHdr, fpool)
            == crate::tr::exit_area::FROM_R15 as usize + crate::tr::exit_area::SIZE
    );
    assert!(std::mem::align_of::<BudgetHdr>() == 8);
    assert!(std::mem::offset_of!(BudgetHdr, fpool) < i32::MAX as usize / 2);
};

#[repr(C)]
pub(crate) struct JitData {
    /// Must stay first: `r15` points here (budget word + exit area).
    pub(crate) hdr: BudgetHdr,
    /// P2-3b: the block was mapped near the poll word by `alloc_data` (freed with `munmap`, not as a `Box`).
    near: bool,
    pub(crate) enabled: bool,
    /// The mode the embedder / environment chose for this state ([`set_mode`]); `jit.on()` returns to it, so a script
    /// can never enable a compiler its host switched off.
    base_mode: Mode,
    /// `on` mode heuristics (skip functions the templates barely help, entry back-off); `always`/`verify`
    /// compile everything and stay deterministic.
    heuristic: bool,
    pub(crate) arena: CodeArena,
    thunk: Option<(ThunkEx, CodeBlock)>,
    /// Addresses of the live descriptors (`Box::into_raw` pointers); the remaining ones are freed at close.
    pub(crate) live: HashSet<usize>,
    /// Debug builds: descriptors of collected prototypes, kept (poisoned) so a stale use is caught by an
    /// assertion instead of being a use-after-free. Freed when the state closes.
    quarantine: Vec<*mut JitProto>,
    /// Layout facts resolved once per state.
    env: Option<CallEnv>,
    /// BUNDLE-T1I: per-prototype call-site masks of the inline built-ins (key: address of the code), removed with the descriptor.
    pub(crate) t1i_hints: crate::lower::t1inline::SiteHints,
    /// After a transient refusal the next compile requests are refused without work.
    pub(crate) deny: u32,
    /// (The block-count budget word, `hdr.cnt`, is reseeded from `budget` at every entry.)
    budget: i32,
    pub(crate) stats: Stats,
    /// `RUSTYNAIL_JIT_VERBOSE=1`: exit/compile profile, printed when the state closes.
    prof: Option<Box<Prof>>,
    /// `RUSTYNAIL_JIT_T15` / `_T15_HOT` / `JIT_OPT` of this state (overridable by tests, see [`set_t15`]).
    pub(crate) t15: T15Config,
    /// J5 trace cache and configuration of this state (docs/JIT_TRACE.md section 3).
    #[cfg(feature = "trace")]
    pub(crate) tr: crate::tr::cache::TrState,
}

/// Per-state exit profile (`RUSTYNAIL_JIT_VERBOSE=1`): which instruction compiled code leaves at, how
/// often, and how many cycles (rdtsc) each stay in compiled code lasted. Off by default (one `Option`
/// test per entry).
#[derive(Default)]
struct Prof {
    /// (proto code address, exit pc index) -> (exits, cycles in compiled code before them)
    exits: std::collections::HashMap<(usize, u32), (u64, u64)>,
    /// proto code address -> description, entries
    protos: std::collections::HashMap<usize, ProtoProf>,
}

#[derive(Default)]
struct ProtoProf {
    line: i32,
    total: usize,
    templates: usize,
    loop_total: usize,
    loop_templates: usize,
    entry_ok: bool,
    interp_entry: bool,
    code_bytes: usize,
    enters: u64,
    cycles: u64,
    insns: u64,
    blocks: u32,
    code: Vec<u32>,
}

pub(crate) fn verbose() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("RUSTYNAIL_JIT_VERBOSE").is_ok_and(|v| v != "0"))
}

impl Prof {
    fn report(&self, stats: &Stats) {
        eprintln!(
            "[jit] compiled {} failed {} refused {} enters {} (osr {}) exits: continue {} frame_changed {} returned {} code {} B",
            stats.compiled, stats.failed, stats.refused, stats.enters, stats.osr_enters,
            stats.exit_continue, stats.exit_frame_changed, stats.exit_returned, stats.code_bytes
        );
        if stats.t15_promoted + stats.t15_recompiled + stats.t15_refused + stats.t15_off > 0 {
            eprintln!(
                "[jit] t15: promoted {} recompiled {} refused {} off {}",
                stats.t15_promoted, stats.t15_recompiled, stats.t15_refused, stats.t15_off
            );
            eprintln!(
                "[jit] t15 policy: regions {} stub_events {} respecialised {} given_up {} penalties {}",
                stats.t15_regions,
                stats.t15_stub_events,
                stats.t15_respec,
                stats.t15_giveups,
                stats.t15_penalties
            );
            let (checked, inconclusive) = crate::t15::verify::shadow_counts();
            if checked + inconclusive > 0 {
                eprintln!("[jit] t15 shadow: checked {checked} inconclusive {inconclusive}");
            }
        }
        let mut ps: Vec<_> = self.protos.iter().collect();
        ps.sort_by_key(|(_, v)| std::cmp::Reverse(v.cycles));
        for (a, v) in ps.iter().take(12) {
            eprintln!(
                "[jit] proto@{a:x} line {} n={} tmpl={} loop {}/{} entry_ok={} interp_entry={} {}B enters={} insns/enter={:.1} blocks={} Mcyc_in_jit={:.1}",
                v.line, v.total, v.templates, v.loop_templates, v.loop_total, v.entry_ok,
                v.interp_entry, v.code_bytes, v.enters, v.insns as f64 / v.enters.max(1) as f64, v.blocks, v.cycles as f64 / 1e6
            );
        }
        let mut es: Vec<_> = self.exits.iter().collect();
        es.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
        for ((a, pc), (n, cyc)) in es.iter().take(25) {
            let line = self.protos.get(a).map_or(0, |p| p.line);
            let w = self
                .protos
                .get(a)
                .and_then(|p| p.code.get(*pc as usize))
                .copied()
                .unwrap_or(0);
            let op = rn_core::instruction::Instruction(w).opcode();
            eprintln!(
                "[jit] exit proto@{a:x} line {line} pc {pc} {op:?}: {n} exits, {:.1} cyc/stay",
                *cyc as f64 / *n as f64
            );
        }
    }
}

impl JitData {
    fn new(mode: Mode) -> JitData {
        let enabled = mode != Mode::Off && probe_exec().is_ok();
        let mut arena = CodeArena::new();
        if mode == Mode::Verify {
            arena.set_quarantine(true);
        }
        // test/ops knob: cap on mapped code bytes (default 256 MiB); a cap that is hit refuses compiles
        // (transient, the function stays in the interpreter) until freed code makes room again
        if let Some(cap) = std::env::var("RUSTYNAIL_JIT_CODE_CAP")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            arena.set_cap(cap);
        }
        if let Some(m) = PerfMap::from_env() {
            arena.set_perf_map(Some(m));
        }
        JitData {
            near: false,
            enabled,
            base_mode: mode,
            heuristic: mode == Mode::On,
            arena,
            thunk: None,
            live: HashSet::new(),
            quarantine: Vec::new(),
            env: None,
            t1i_hints: Default::default(),
            deny: 0,
            hdr: BudgetHdr {
                cnt: i32::MAX,
                _pad: 0,
                #[cfg(feature = "trace")]
                area: crate::tr::ExitArea::default(),
                #[cfg(feature = "trace")]
                fpool: crate::tr::FPool::new(),
            },
            budget: i32::MAX,
            stats: Stats::default(),
            prof: verbose().then(Box::default),
            t15: {
                #[cfg_attr(not(feature = "trace"), allow(unused_mut))]
                let mut c = T15Config::process();
                // J5/F5 (TR19): with traces on, J4's regions are the fallback owner of nests the tracer gave up; they
                // default to `on` together with traces unless `RUSTYNAIL_JIT_T15` says otherwise.
                #[cfg(feature = "trace")]
                if c.mode == T15Mode::Off
                    && std::env::var_os("RUSTYNAIL_JIT_T15").is_none()
                    && crate::tr::cache::TrConfig::process().mode != crate::tr::cache::TrMode::Off
                {
                    c = T15Config::parse(
                        Some("on"),
                        std::env::var("RUSTYNAIL_JIT_T15_HOT").ok().as_deref(),
                        std::env::var("RUSTYNAIL_JIT_OPT").ok().as_deref(),
                    );
                }
                if verbose() && !c.opt.unknown.is_empty() {
                    eprintln!("[t15] unknown RUSTYNAIL_JIT_OPT items: {:?}", c.opt.unknown);
                }
                c
            },
            #[cfg(feature = "trace")]
            tr: crate::tr::cache::TrState::new(crate::tr::cache::TrConfig::process()),
        }
    }
}

#[inline]
pub(crate) fn data_ptr(st: &mut State) -> *mut JitData {
    let slot = jit_hooks::slot(st);
    if slot.data.is_null() {
        init_data(slot);
    }
    slot.data.cast()
}

/// First use of the per-state data. P-J: out of line; inlined into `data_ptr` it gave the function a 6 KB frame (stack
/// probe, six pushes) that every helper call paid (8% of `knucleotide` was the prologue of `data_ptr`).
#[cold]
#[inline(never)]
fn init_data(slot: &mut jit_hooks::JitSlot) {
    let mode = CONFIG.get().map_or(Mode::Off, |c| c.mode);
    slot.data = alloc_data(JitData::new(mode)).cast();
}

/// P2-3b: the per-state data is placed within reach of the poll word (`TRACE_POLL`: a static of the binary) when the address space has room
/// there, so that traces poll with `cmp byte [r15 + disp], 0` (no `movabs` of the address). Mapped anonymously with the address of the static as a
/// hint (tried at growing distances on both sides); too far (or no mapping): an ordinary `Box` (the traces then build the address in a register).
/// The block is freed by [`DataGuard`] (`JitData::near`).
fn alloc_data(d: JitData) -> *mut JitData {
    #[cfg(all(unix, feature = "trace"))]
    {
        let size = std::mem::size_of::<JitData>().next_multiple_of(4096);
        let target = jit_hooks::trace_poll_flag_addr();
        // 2^31 minus the block and the pool: displacements stay below 2 GiB
        let reach = (1usize << 31) - (1usize << 24) - size;
        for k in 1..=96usize {
            let step = (k / 2) * (1 << 23);
            let hint = if k % 2 == 0 {
                target.wrapping_add(step + (1 << 22))
            } else {
                target.wrapping_sub(step + size + (1 << 22))
            };
            // SAFETY: an anonymous private mapping; the kernel treats a nonzero address as a hint.
            let p = unsafe {
                libc::mmap(
                    (hint & !0xFFF) as *mut libc::c_void,
                    size,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                break;
            }
            if (p as usize).abs_diff(target) < reach {
                let p = p.cast::<JitData>();
                // SAFETY: fresh, page-aligned, large enough, exclusively ours.
                unsafe {
                    p.write(d);
                    (*p).near = true;
                }
                lsan_root(p.cast(), size, true);
                return p;
            }
            // SAFETY: the mapping just made.
            unsafe { libc::munmap(p, size) };
        }
    }
    Box::into_raw(Box::new(d))
}

/// LeakSanitizer builds (`--cfg rustynail_lsan`, `scripts/asan*.sh`): the per-state data is an anonymous mapping (above), which LeakSanitizer
/// does not scan (it scans globals, stacks, registers and heap chunks only), so every heap block reachable only through it (the trace cache, the
/// exit tables, the descriptors) was reported as leaked when the process exits with the state still open (`os.exit`, a script that ends with it:
/// jit-diff seeds 3, 17, 20, 32, 37). A `Box` would be found through the state; this registers the mapping as a root region for the time it
/// lives, which gives the same reachability. After a normal close the region is unregistered before the unmap, so a block that the drop of the
/// data failed to free is still reported.
#[cfg(rustynail_lsan)]
fn lsan_root(p: *mut u8, size: usize, register: bool) {
    extern "C" {
        fn __lsan_register_root_region(p: *const std::ffi::c_void, size: usize);
        fn __lsan_unregister_root_region(p: *const std::ffi::c_void, size: usize);
    }
    // SAFETY: the interface functions of the sanitizer runtime; `p` / `size` describe the live mapping.
    unsafe {
        if register {
            __lsan_register_root_region(p.cast(), size);
        } else {
            __lsan_unregister_root_region(p.cast(), size);
        }
    }
}

#[cfg(not(rustynail_lsan))]
#[inline(always)]
fn lsan_root(_p: *mut u8, _size: usize, _register: bool) {}

/// Owner of the per-state data at its end of life: derefs to it and frees it as [`alloc_data`] allocated it.
struct DataGuard(*mut JitData);

impl std::ops::Deref for DataGuard {
    type Target = JitData;
    fn deref(&self) -> &JitData {
        // SAFETY: the pointer is the live per-state data, owned by this guard.
        unsafe { &*self.0 }
    }
}

impl Drop for DataGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `alloc_data`, dropped once here.
        unsafe {
            if (*self.0).near {
                let size = std::mem::size_of::<JitData>().next_multiple_of(4096);
                self.0.drop_in_place();
                lsan_root(self.0.cast(), size, false);
                #[cfg(unix)]
                libc::munmap(self.0.cast(), size);
            } else {
                drop(Box::from_raw(self.0));
            }
        }
    }
}

/// The address of the block-count budget word (`r15`) of `st` (shadow verification, `t15::verify`).
pub(crate) fn cnt_ptr(st: &mut State) -> *mut i32 {
    // SAFETY: `data_ptr` returns the live per-state data; only the address of the header (provenance over the word
    // and, with `trace`, the exit area behind it) is taken.
    unsafe { (&raw mut (*data_ptr(st)).hdr).cast::<i32>() }
}

/// P2-3c: the address of the double-constant pool of `st`.
#[cfg(feature = "trace")]
pub(crate) fn fpool_ptr(st: &mut State) -> *mut crate::tr::FPool {
    // SAFETY: the per-state data is live; only an address is formed.
    unsafe { (&raw mut (*data_ptr(st)).hdr.fpool).cast() }
}

/// P2-3c: displacement of `FPool::bits[0]` from `r15`.
#[cfg(feature = "trace")]
pub(crate) fn fpool_off() -> i32 {
    (std::mem::offset_of!(BudgetHdr, fpool) + std::mem::offset_of!(crate::tr::FPool, bits)) as i32
}

/// The budget word (`r15`) of `st`, with provenance over the exit area behind it (tests: the real area that
/// `tr::exit::jit_tr_exit` reads).
#[cfg(feature = "trace")]
#[doc(hidden)]
pub fn trace_budget_ptr(st: &mut State) -> *mut i32 {
    cnt_ptr(st)
}

/// Like [`data_ptr`] but only when the compiler is installed: without a registered `free_state` callback the
/// per-state data would leak, so the test-support entry points do nothing in that case.
fn data_ptr_registered(st: &mut State) -> Option<*mut JitData> {
    jit_hooks::vtable().map(|_| data_ptr(st))
}

fn free_state(p: *mut ()) {
    // SAFETY: `p` came from `alloc_data` in `data_ptr` and the slot is dropped exactly once.
    let d = DataGuard(p.cast::<JitData>());
    if let Some(pr) = &d.prof {
        pr.report(&d.stats);
    }
    #[cfg(feature = "trace")]
    crate::tr::cache::report(&d);
    // descriptors of prototypes that outlived their state's heap (cannot normally happen: the heap is
    // dropped first and frees them through `free_code`); the arena's drop unmaps the code
    for &a in &d.live {
        // SAFETY: every entry is a live `Box::into_raw` descriptor not freed elsewhere.
        drop(unsafe { Box::from_raw(a as *mut JitProto) });
    }
    for &a in &d.quarantine {
        // SAFETY: quarantined descriptors are `Box::into_raw` pointers from `free_code`, dropped once here
        // (`CodeBlock` has no `Drop`: its code was already returned to the arena).
        drop(unsafe { Box::from_raw(a) });
    }
}

/// `VTable::free_code`: the prototype owning descriptor `d` is being collected. Returns the bytes to
/// refund.
fn free_code(d: *mut u8) -> usize {
    // SAFETY: `d` is a descriptor created by `compile` (`Box::into_raw`), still registered in its owner.
    let mut jp = unsafe { Box::from_raw(d.cast::<JitProto>()) };
    // SAFETY: the owner outlives its protos (the heap, which frees them, is dropped before the slot).
    let owner = unsafe { &mut *jp.owner };
    owner.live.remove(&(d as usize));
    owner.t1i_hints.remove(&(jp.code_ptr as usize));
    owner.stats.freed += 1;
    // J5: flush the traces rooted at this prototype and those that inline it (TR32/TR18) *before* the descriptor and
    // its dependency lists go away; their charges are refunded with the prototype's.
    #[cfg(feature = "trace")]
    let tr_refund = match jp.tr.as_deref() {
        Some(tp) => crate::tr::cache::on_proto_free(owner, d as usize, tp),
        None => 0,
    };
    #[cfg(feature = "trace")]
    crate::tr::cache::verify_landings(owner);
    #[cfg(not(feature = "trace"))]
    let tr_refund = 0;
    let charged = jp.charged + tr_refund;
    jp.dead.set(true);
    // Retired code versions (tier 1.5) are freed together with the current block: while the prototype lives
    // they stay mapped, because a helper called from an old version may still return into it.
    let retired = jp
        .t15
        .as_mut()
        .map(|t| std::mem::take(&mut t.retired))
        .unwrap_or_default();
    for b in retired {
        // SAFETY: as below: no frame of this prototype exists, so nothing executes in any of its versions.
        let _ = unsafe { owner.arena.free(b) };
    }
    // A collected prototype has no live frame (a running frame keeps its closure, hence the proto, alive), so
    // nothing executes in its block. A stay of *this* descriptor that is still running (it entered the proto
    // earlier, the frame returned flat into a compiled caller, which called the collector) only returns
    // through the thunk: `enter` holds no pointer to the descriptor, the proto or the frame across the call.
    let block = if cfg!(debug_assertions) {
        // SAFETY: `CodeBlock` is plain data without `Drop`; this copy is the only one handed to `free`,
        // the descriptor's own copy is never used again (`dead`).
        unsafe { std::ptr::read(&raw const jp.block) }
    } else {
        let JitProto { block, .. } = *jp;
        // SAFETY: as above, nothing executes in the block.
        let _ = unsafe { owner.arena.free(block) };
        return charged;
    };
    // SAFETY: nothing executes in the block (see above).
    let _ = unsafe { owner.arena.free(block) };
    // debug builds keep the poisoned descriptor until the state closes, so a stale use trips an assertion
    owner.quarantine.push(Box::into_raw(jp));
    charged
}

/// Overrides the mode of one state (tests: a jit-off twin next to jit-on states in one process).
pub fn set_mode(st: &mut State, mode: Mode) {
    let Some(dp) = data_ptr_registered(st) else {
        return;
    };
    // SAFETY: as below.
    unsafe { (*dp).base_mode = mode };
    apply_mode(st, mode);
}

/// [`set_mode`] without recording `mode` as the state's base mode (`jit.off()` / `jit.on()`).
fn apply_mode(st: &mut State, mode: Mode) {
    let Some(dp) = data_ptr_registered(st) else {
        return;
    };
    // SAFETY: `data_ptr` returns the live per-state data; no other reference exists.
    let d = unsafe { &mut *dp };
    #[cfg(feature = "trace")]
    let was_enabled = d.enabled;
    d.enabled = mode != Mode::Off && probe_exec().is_ok();
    #[cfg(feature = "trace")]
    if !d.enabled {
        crate::tr::cache::flush_all(st, crate::tr::cache::FlushWhy::Off);
    }
    // SAFETY: as above; `flush_all` re-derives its own reference and keeps none.
    let d = unsafe { &mut *dp };
    #[cfg(feature = "trace")]
    if d.enabled && !was_enabled {
        crate::tr::cache::rearm_all(d); // J5: heads parked while the compiler was off count again
    }
    d.heuristic = mode == Mode::On;
    if mode == Mode::Verify {
        d.arena.set_quarantine(true);
    }
    if matches!(mode, Mode::Always | Mode::Verify) {
        // thresholds are process-wide and baked into prototypes created afterwards
        jit_hooks::set_thresholds(1, 1);
    }
}

/// `jit.status()`: `Some((enabled, features))` when the compiler is installed for this process, else `None`.
fn ctl_status(st: &mut State) -> Option<(bool, Vec<&'static str>)> {
    let dp = data_ptr_registered(st)?;
    // SAFETY: as in `set_mode`; read-only.
    let d = unsafe { &*dp };
    let mut f = vec!["tier1"];
    #[cfg(feature = "trace")]
    if d.tr.cfg.mode != crate::tr::cache::TrMode::Off {
        f.push("trace");
    }
    f.push(if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "native"
    });
    Some((d.enabled, f))
}

/// `jit.on()` / `jit.off()`: the state's base mode (or `Off`). A no-op when the compiler is not installed
/// (`RUSTYNAIL_JIT=off`). Turning off flushes the traces (deferred while an exit handler runs) and stops entering
/// compiled code; code already running keeps running.
fn ctl_set_enabled(st: &mut State, on: bool) {
    let Some(dp) = data_ptr_registered(st) else {
        return;
    };
    // SAFETY: as in `set_mode`; read-only.
    let base = unsafe { (*dp).base_mode };
    // `jit.on()` restores the mode the host chose (a no-op when the host runs this state without the compiler)
    apply_mode(st, if on { base } else { Mode::Off });
}

/// `jit.flush()`: drops every trace of the state through the established safe path (an active recording is aborted,
/// flushes requested inside an exit handler are deferred, compiled code on the stack stays valid until retired).
/// Tier-1 code of prototypes is valid regardless of traces and is kept.
fn ctl_flush(st: &mut State) {
    if data_ptr_registered(st).is_none() {
        return;
    }
    #[cfg(feature = "trace")]
    {
        crate::tr::cache::control_flush(st);
    }
}

/// Sets the cap on mapped code bytes of `st`'s arena (default 256 MiB, or `RUSTYNAIL_JIT_CODE_CAP`). When the
/// cap is hit compiles are refused as transient failures (`Stats::refused`) and the function runs in the
/// interpreter; freed code space is reused, so compiling resumes once prototypes were collected.
pub fn set_code_cap(st: &mut State, cap: usize) {
    if let Some(dp) = data_ptr_registered(st) {
        // SAFETY: as in `set_mode`.
        unsafe { (*dp).arena.set_cap(cap) };
    }
}

/// Overrides the tier 1.5 configuration of one state (tests: `RUSTYNAIL_JIT_T15` is process-wide, this makes a
/// jit-on twin with and without promotion possible in one process). Applies to prototypes compiled afterwards.
pub fn set_t15(st: &mut State, cfg: T15Config) {
    if let Some(dp) = data_ptr_registered(st) {
        // SAFETY: as in `set_mode`.
        unsafe { (*dp).t15 = cfg };
    }
}

/// Overrides the J5 trace configuration of one state (tests: `RUSTYNAIL_JIT_TRACE` is process-wide). Applies to
/// prototypes compiled afterwards (the dispatch cells are part of the tier-1 code); switching an active state `Off`
/// flushes its traces (docs/JIT_TRACE.md 3.4) and stops recording.
#[cfg(feature = "trace")]
pub fn set_trace(st: &mut State, cfg: crate::tr::cache::TrConfig) {
    if let Some(dp) = data_ptr_registered(st) {
        let cfg = cfg.checked();
        let off = cfg.mode == crate::tr::cache::TrMode::Off;
        // SAFETY: as in `set_mode`.
        let was_off = unsafe { (*dp).tr.cfg.mode == crate::tr::cache::TrMode::Off };
        // SAFETY: as in `set_mode`.
        unsafe { (*dp).tr.cfg = cfg };
        if off {
            crate::tr::cache::flush_all(st, crate::tr::cache::FlushWhy::Off);
        } else if was_off {
            // SAFETY: as in `set_mode`.
            crate::tr::cache::rearm_all(unsafe { &*dp }); // heads parked while off count again
        }
    }
}

/// Facade switch (F9, `Builder::jit_trace` / `Lua::set_jit_trace`): enables (`true`: the process configuration of
/// `RUSTYNAIL_JIT_TRACE`, which is `on` unless the environment says otherwise) or disables (`false`: J4 behaviour)
/// traces for one state. Returns whether traces can run for `st` afterwards: false when disabled, when the compiler
/// is not installed, or when the process turned traces off with `RUSTYNAIL_JIT_TRACE=off` (the pipeline driver is
/// then not installed, like `RUSTYNAIL_JIT=off` for the whole JIT).
#[cfg(feature = "trace")]
pub fn set_trace_enabled(st: &mut State, on: bool) -> bool {
    let mut cfg = crate::tr::cache::TrConfig::process();
    if !on {
        cfg.mode = crate::tr::cache::TrMode::Off;
    }
    let process_on = crate::tr::cache::TrConfig::process().mode != crate::tr::cache::TrMode::Off;
    set_trace(st, cfg);
    on && process_on && jit_hooks::vtable().is_some()
}

/// The J5 trace configuration of `st` (the pipeline driver reads the options and thresholds from it).
#[cfg(feature = "trace")]
pub fn trace_config(st: &mut State) -> crate::tr::cache::TrConfig {
    let dp = data_ptr(st);
    // SAFETY: as in `set_mode`.
    unsafe { (*dp).tr.cfg.clone() }
}

/// Is the compiler installed and able to run on this host (executable memory available)?
pub fn available() -> bool {
    jit_hooks::vtable().is_some() && probe_exec().is_ok()
}

/// Sets the instruction budget word compiled code starts with (tests: a small value forces block-count
/// exits; the default is `i32::MAX`).
pub fn set_count_budget(st: &mut State, budget: i32) {
    if let Some(dp) = data_ptr_registered(st) {
        // SAFETY: as in `set_mode`.
        unsafe { (*dp).budget = budget };
    }
}

/// Counters of `st`'s global state.
pub fn stats(st: &mut State) -> Stats {
    match data_ptr_registered(st) {
        // SAFETY: as in `set_mode`.
        Some(dp) => unsafe {
            let d = &*dp;
            Stats {
                live: d.live.len() as u64,
                mapped: d.arena.mapped() as u64,
                sealed: d.arena.sealed_bytes() as u64,
                ..d.stats
            }
        },
        None => Stats::default(),
    }
}

pub(crate) fn proto_of(st: &State, ci: Ci) -> Option<rn_core::value::Gc<rn_core::func::LClosure>> {
    st.get(ci.r().func).as_lclosure()
}

/// Why a compile attempt failed.
#[derive(PartialEq, Eq)]
pub(crate) enum Fail {
    /// This prototype will never compile (unsupported shape, assembler error, W^X refused).
    Permanent,
    /// Transient: code cap or memory limit; the prototype may be retried later (after code was freed).
    Retry,
}

/// Compiles `p`; on a permanent failure marks it do-not-compile. True when compiled code exists afterwards.
pub(crate) fn ensure_compiled(st: &mut State, p: &Proto) -> bool {
    match p.jit.get() as usize {
        0 => {}
        NOJIT => return false,
        _ => return true,
    }
    match compile(st, p) {
        Ok(()) => true,
        Err(Fail::Retry) => {
            // SAFETY: as in `set_mode`.
            unsafe { (*data_ptr(st)).stats.refused += 1 };
            false
        }
        Err(Fail::Permanent) => {
            p.jit.set(NOJIT as *mut u8);
            // SAFETY: as in `set_mode`.
            unsafe { (*data_ptr(st)).stats.failed += 1 };
            false
        }
    }
}

fn round_page(n: usize) -> usize {
    let pg = page_size();
    n.div_ceil(pg) * pg
}

/// Seals the reopened page again; if that fails the code on it cannot run, which cannot be recovered.
fn reseal_or_abort(d: &mut JitData) {
    if d.arena.commit().is_err() {
        eprintln!("rn-jit: cannot seal a code page again (mprotect failed); aborting");
        std::process::abort();
    }
}

pub(crate) fn call_env(st: &mut State, d: &mut JitData) -> CallEnv {
    if let Some(mut e) = d.env {
        e.tail_fast = tail_fast(d);
        e.armm = armm(d);
        e.tail_mut = tail_mut();
        (e.idx_name, e.idx_hash, e.idx_levels) = idx_name(st, d);
        e.tset = tset_bits(d);
        e.str_mt = str_mt_addr(st, d);
        (e.inl, e.inl_mut) = t1inline_bits(d);
        e.t1q = t1q_bits(d);
        e.ccall_helper = ccall_helper(d);
        e.hints = &d.t1i_hints;
        return e;
    }
    let pl = rn_core::jit_layout::proto_layout_probe();
    let e = CallEnv {
        stack_off: jit_hooks::thread::stack_ptr_offset(st) as i32,
        proto_code: pl.code_ptr as i32,
        proto_k: pl.k_ptr as i32,
        call_helper: jit_helpers::jit_call as *const () as usize as u64,
        ccall_helper: 0,
        tail_helper: jit_helpers::jit_tailcall as *const () as usize as u64,
        tail_fast: false,
        armm: false,
        tail_mut: 0,
        idx_name: 0,
        idx_hash: 0,
        idx_levels: 0,
        tset: 0,
        str_mt: 0,
        inl: 0,
        inl_mut: 0,
        t1q: 0,
        hints: std::ptr::null(),
    };
    d.env = Some(e);
    let (idx_name, idx_hash, idx_levels) = idx_name(st, d);
    let (inl, inl_mut) = t1inline_bits(d);
    CallEnv {
        tail_fast: tail_fast(d),
        armm: armm(d),
        tail_mut: tail_mut(),
        idx_name,
        idx_hash,
        idx_levels,
        tset: tset_bits(d),
        str_mt: str_mt_addr(st, d),
        inl,
        inl_mut,
        t1q: t1q_bits(d),
        hints: &d.t1i_hints,
        ccall_helper: ccall_helper(d),
        ..e
    }
}

/// R1: the mutants of the inline tail call (`calls::TM_*`).
fn tail_mut() -> u8 {
    #[cfg(feature = "trace")]
    {
        crate::tr::tail_mutants()
    }
    #[cfg(not(feature = "trace"))]
    {
        0
    }
}

/// T3: the interned `"__index"` string (address, hash) for the inline `__index` level of the table templates; `(0, 0)` = off. With
/// traces on only (`t3idx` limit), so that `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly. The name is a fixed string of the heap and
/// the code is only run on threads of the same global state.
fn idx_name(st: &State, d: &JitData) -> (u64, u32, u8) {
    #[cfg(feature = "trace")]
    {
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off && d.tr.cfg.limits.t3idx != 0 {
            let s = st.g().heap.tm_name(rn_core::meta::Tm::Index);
            return (
                s.as_ptr() as usize as u64,
                s.get().hash(),
                d.tr.cfg.limits.t3idx as u8,
            );
        }
    }
    let _ = (st, d);
    (0, 0, 0)
}

/// BUNDLE-K item B: the address of the string-metatable slot `g.mt[LUA_TSTRING]` (0 = off; also off without the `__index` name of T3).
/// `g` is pinned for the life of the state and the compiled code only runs on threads of the same global state.
fn str_mt_addr(st: &State, d: &JitData) -> u64 {
    #[cfg(feature = "trace")]
    {
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off && d.tr.cfg.limits.t3idx != 0 {
            return &st.g().mt[rn_core::value::LUA_TSTRING as usize] as *const _ as usize as u64;
        }
    }
    let _ = (st, d);
    0
}

/// BUNDLE-C (`t1ccall`): the address of the lean light-C call helper, 0 = off (the limit; also with traces off when it is named in
/// `RUSTYNAIL_JIT_TRACE_LIMITS`: the process default is 0, so `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly unless asked).
fn ccall_helper(d: &JitData) -> u64 {
    #[cfg(feature = "trace")]
    {
        if d.tr.cfg.limits.t1ccall != 0 {
            return jit_helpers::jit_ccall as *const () as usize as u64;
        }
    }
    let _ = d;
    0
}

/// BUNDLE-T1I: the built-ins the tier-1 `CALL` template runs inline (`t1inline` limit, restricted to the rows that exist in this
/// process) and the test-only mutants; with traces on only, so that `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly.
fn t1inline_bits(d: &JitData) -> (u16, u16) {
    #[cfg(feature = "trace")]
    {
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off {
            return (
                d.tr.cfg.limits.t1inline as u16 & crate::lower::t1inline::available(),
                crate::tr::t1inline_mut(),
            );
        }
    }
    let _ = d;
    (0, 0)
}

/// BUNDLE-Q (`t1q`): the tier-1 exits removed (bits 0-1 of the limit) plus the test-only mutants (bits 8..). With traces off only when the limit was named.
fn t1q_bits(d: &JitData) -> u32 {
    #[cfg(feature = "trace")]
    {
        let l = d.tr.cfg.limits.t1q;
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off || l & 0x80 != 0 {
            return (l & 0x7f) | crate::tr::t1q_mut();
        }
    }
    let _ = d;
    0
}

/// BUNDLE-B item 2: the inline store paths of tier 1 (`tset` limit); with traces on only, so that `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly.
fn tset_bits(d: &JitData) -> u8 {
    #[cfg(feature = "trace")]
    {
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off {
            return d.tr.cfg.limits.tset as u8;
        }
    }
    let _ = d;
    0
}

/// BUNDLE-D (`armm`): tier 1 continues in compiled code after the Lua metamethod of an arithmetic instruction returns (with traces on only).
fn armm(d: &JitData) -> bool {
    #[cfg(feature = "trace")]
    {
        let l = &d.tr.cfg.limits;
        // (BUNDLE-V: `t1q` bit 4 also applies with traces off when the limit was named, as `t1q_bits`)
        (d.tr.cfg.mode != crate::tr::cache::TrMode::Off && l.armm != 0)
            || (l.t1q & 16 != 0
                && (d.tr.cfg.mode != crate::tr::cache::TrMode::Off || l.t1q & 0x80 != 0))
    }
    #[cfg(not(feature = "trace"))]
    {
        let _ = d;
        false
    }
}

/// R1: the inline tail call of tier 1 (R1): with traces on only, so that `RUSTYNAIL_JIT_TRACE=off` stays J4 exactly.
fn tail_fast(d: &JitData) -> bool {
    #[cfg(feature = "trace")]
    {
        d.tr.cfg.mode != crate::tr::cache::TrMode::Off
    }
    #[cfg(not(feature = "trace"))]
    {
        let _ = d;
        false
    }
}

/// Rust-heap bytes the compiler holds *while* compiling `p` (lowering buffers: micro-ops, resume list, label
/// table, emitted code). An estimate from the instruction count (a template emits a few micro-ops, 16 B each,
/// and a few dozen bytes of code); charged to the memory limit for the duration of the compile only, so a tight
/// limit refuses the compile (the function stays in the interpreter) instead of the host allocating past it.
fn transient_bytes(p: &Proto) -> usize {
    p.code().len() * 256
}

/// Rust-heap bytes the descriptor of `p` keeps for its whole life (charged with the code, refunded when
/// the prototype is freed): the `JitProto` box, its resume table, the monitor, and the live-set slot.
fn descriptor_bytes(p: &Proto) -> usize {
    std::mem::size_of::<JitProto>()
        // (the `tr` pointer of the trace build is charged with the trace table, `TrProto::bytes_for`, so that a state
        // with traces off is charged exactly what the build without the feature charges)
        - if cfg!(feature = "trace") { std::mem::size_of::<usize>() } else { 0 }
        + std::mem::size_of_val(p.code())
        + std::mem::size_of::<Mon>()
        + 2 * std::mem::size_of::<usize>() // Rc counts
        + 2 * std::mem::size_of::<usize>() // HashSet slot (amortised)
}

/// Compiles `p`, charging the descriptor and the compile's transient buffers to the memory limit
/// (docs/JIT.md "Memory"): a refused charge is a transient failure.
fn compile(st: &mut State, p: &Proto) -> Result<(), Fail> {
    // SAFETY: as in `set_mode`.
    let d0 = unsafe { &*data_ptr(st) };
    if !d0.enabled {
        return Err(Fail::Permanent);
    }
    // tier 1.5: the loop nests of the prototype (only when the knob is on); their bookkeeping is charged with
    // the descriptor and refunded with it
    // (F5, TR19: with traces on the nests are scanned as well: J4's promote stub `W_H` is the fallback owner of a nest
    // the tracer blacklisted)
    let nests = if d0.t15.mode != T15Mode::Off {
        t15rt::scan_nests(p.code())
    } else {
        Vec::new()
    };
    let mut desc = descriptor_bytes(p);
    if !nests.is_empty() {
        desc += t15rt::data_bytes(p.code().len(), nests.len());
    }
    let transient = transient_bytes(p);
    if !jit_hooks::charge_code(st, desc + transient) {
        // SAFETY: as in `set_mode`.
        unsafe { (*data_ptr(st)).deny = 32 };
        return Err(Fail::Retry);
    }
    let r = compile_inner(st, p, desc, &nests);
    jit_hooks::uncharge_code(st, transient);
    if r.is_err() {
        jit_hooks::uncharge_code(st, desc);
    }
    r
}

/// Code placed in the arena by [`place_code`].
pub(crate) struct Placed {
    pub(crate) block: CodeBlock,
    /// Bytes of the arena that stay charged for this block (the rest of the up-front charge was refunded).
    pub(crate) own: usize,
}

/// Charges, allocates, writes and commits `code` (and the entry thunk when this is the state's first
/// compile). On any failure everything is undone: the page is sealed again, the charge refunded, the
/// never-executed blocks freed. The page-sealing protocol is J-PACK: no generated code runs in between.
pub(crate) fn place_code(
    st: &mut State,
    dp: *mut JitData,
    code: &[u8],
    align: usize,
) -> Result<Placed, Fail> {
    // SAFETY: `dp` is the live per-state data (see `set_mode`).
    let d = unsafe { &mut *dp };
    let thunk_code = if d.thunk.is_none() {
        Some(x86::thunk_code())
    } else {
        None
    };
    // What the code costs: an upper bound is charged up front (whole pages), the difference to what was
    // really sealed is refunded after the commit. Compiles are packed into the partially used last page
    // of the current chunk (`reopen_tail`), so small functions cost about their size.
    let thunk_pages = thunk_code.as_ref().map_or(0, |c| round_page(c.len()));
    let want = round_page(code.len()) + thunk_pages;
    if !jit_hooks::charge_code(st, want) {
        d.deny = 32;
        return Err(Fail::Retry); // memory limit: transient (freeing code or objects makes room)
    }
    // SAFETY: `dp` is still the live per-state data (charge_code did not touch it).
    let d = unsafe { &mut *dp };
    let sealed_before = d.arena.sealed_bytes();
    // A compile is synchronous and runs no generated code; the page is sealed again by the commit below
    // on every path (docs/UNSAFE.md, J-PACK).
    let reopened = d.arena.reopen_tail();
    let fail = |st: &mut State, d: &mut JitData, blocks: Vec<CodeBlock>, refund: usize| {
        for b in blocks {
            // SAFETY: never executed (not committed, or the compile failed before publishing it).
            let _ = unsafe { d.arena.free(b) };
        }
        if reopened {
            reseal_or_abort(d);
        }
        jit_hooks::uncharge_code(st, refund);
    };
    let mut block = match d.arena.alloc(code.len(), align) {
        Ok(b) => b,
        Err(e) => {
            fail(st, d, Vec::new(), want);
            if matches!(e, crate::mem::ArenaError::CapExceeded) {
                d.deny = 32;
                return Err(Fail::Retry);
            }
            return Err(Fail::Permanent);
        }
    };
    match d.arena.writer(&mut block) {
        Ok(mut w) => w.write_at(0, code),
        Err(_) => {
            fail(st, d, vec![block], want);
            return Err(Fail::Permanent);
        }
    }
    let mut new_thunk = None;
    if let Some(tcode) = thunk_code {
        let tb = d.arena.alloc(tcode.len(), 16).ok().and_then(|mut tb| {
            let ok = d
                .arena
                .writer(&mut tb)
                .map(|mut w| w.write_at(0, &tcode))
                .is_ok();
            ok.then_some(tb)
        });
        match tb {
            Some(tb) => new_thunk = Some(tb),
            None => {
                fail(st, d, vec![block], want);
                return Err(Fail::Retry);
            }
        }
    }
    if d.arena.commit().is_err() {
        let mut bs = vec![block];
        bs.extend(new_thunk);
        fail(st, d, bs, want);
        return Err(Fail::Permanent);
    }
    if let Some(tb) = new_thunk {
        // SAFETY: the block holds the thunk assembled by `x86::thunk_code` and is committed (RX); `Thunk`
        // is its C signature.
        match unsafe { d.arena.entry::<ThunkEx>(&tb) } {
            Ok(f) => d.thunk = Some((f, tb)),
            Err(_) => {
                fail(st, d, vec![block, tb], want);
                return Err(Fail::Permanent);
            }
        }
    }
    let consumed = d.arena.sealed_bytes().saturating_sub(sealed_before);
    let own = consumed
        .saturating_sub(thunk_pages)
        .max(code.len().next_multiple_of(16))
        .min(round_page(code.len()));
    jit_hooks::uncharge_code(st, want.saturating_sub(own + thunk_pages));
    Ok(Placed { block, own })
}

/// Native offset of the entry for pc 0 of a lowered+emitted function (`None`: not compilable).
fn entry0_offset(low: &Lowered, em: &x86::Emitted) -> Option<usize> {
    low.resume
        .iter()
        .find(|(pc, _)| *pc == 0)
        .and_then(|(_, l)| em.label_off.get(*l as usize).copied().flatten())
}

/// The resume table (native offset + 1 per pc, indexed by pc) of a lowered function.
fn resume_table(n: usize, low: &Lowered, em: &x86::Emitted) -> Vec<u32> {
    let mut resume = vec![0u32; n];
    for (pc, l) in &low.resume {
        if let Some(Some(off)) = em.label_off.get(*l as usize) {
            resume[*pc as usize] = *off as u32 + 1;
        }
    }
    for (pc, l) in &low.resume_mm {
        if let (Some(Some(off)), 0) = (em.label_off.get(*l as usize), resume[*pc as usize]) {
            resume[*pc as usize] = (*off as u32 + 1) | crate::lower::calls::RESUME_MM_ONLY;
        }
    }
    resume
}

fn compile_inner(st: &mut State, p: &Proto, desc: usize, nests: &[(u32, u32)]) -> Result<(), Fail> {
    let dp = data_ptr(st);
    // SAFETY: as in `set_mode`; `st` is not used through another path while `d` is live except
    // `charge_code`, which touches the heap accounting only.
    let d = unsafe { &mut *dp };
    if !d.enabled {
        return Err(Fail::Permanent);
    }
    if d.deny > 0 {
        d.deny -= 1;
        return Err(Fail::Retry);
    }
    let env = call_env(st, d);
    if env.inl != 0 {
        d.t1i_hints
            .entry(p.code_ptr() as usize)
            .or_insert_with(|| crate::lower::t1inline::site_hints(p));
    }
    // tier 1.5 data (address-stable: the warm-up code embeds the addresses of its counters and profile bytes);
    // dropped again if the compile fails (nothing refers to it then)
    let t15data = (!nests.is_empty()).then(|| T15Data::new(p.code().len(), nests, &d.t15));
    #[cfg(feature = "trace")]
    let via_dispatch = d.tr.cfg.mode != crate::tr::cache::TrMode::Off;
    #[cfg(not(feature = "trace"))]
    let via_dispatch = false;
    let warm = t15data.as_ref().map(|t| {
        let mut w = t.warmup(0, &[]);
        w.via_dispatch = via_dispatch;
        w
    });
    let cfg = LowerCfg {
        code_addr: p.code_ptr() as usize,
        interrupt_addr: jit_hooks::interrupt_flag_addr(),
        calls: Some(env),
        consts: p.k(),
        t15_warmup: warm.as_ref(),
    };
    #[cfg(feature = "trace")]
    let t_tier1 = crate::tr::phase::now();
    // J5: with traces on, the tier-1 lowering gets the dispatch hook (TR20); otherwise the plain `lower`, whose
    // output is byte-identical to the build without the feature.
    #[cfg(feature = "trace")]
    let mut dis = (d.tr.cfg.mode != crate::tr::cache::TrMode::Off).then(|| {
        crate::tr::cache::Dispatch::new(
            crate::tr::cache::DispatchCfg {
                hot: d.tr.cfg.hot_loop,
                always: d.tr.cfg.mode == crate::tr::cache::TrMode::Always,
                tracemove: d.tr.cfg.opts.tracemove,
                seed: (p.linedefined as u32) ^ ((p.code().len() as u32) << 16),
                line: p.linedefined,
                fn_root: (d.tr.cfg.limits.fnroot != 0 || d.tr.cfg.limits.recnat != 0)
                    && !p.is_vararg(),
                recnat: d.tr.cfg.limits.recnat != 0,
                lazy: d.tr.cfg.limits.lazyci != 0,
                fn_loops: d.tr.cfg.limits.recany != 0,
                rec_any: d.tr.cfg.limits.recany != 0,
                c_entered: d.tr.centry.contains(&(p.code_ptr() as usize)),
            },
            None,
        )
        .with_nests(nests, d.tr.cfg.pref == crate::tr::cache::TrPref::Region)
        .all_black(d.tr.cfg.opts.allblack)
    });
    #[cfg(feature = "trace")]
    let low = match dis.as_mut() {
        Some(dis) => {
            let mut hook = |cx: &mut crate::lower::Cx<'_>, env: &crate::lower::RegionEnv<'_>| {
                crate::tr::cache::emit_dispatch(cx, env, dis)
            };
            crate::lower::lower_ex(p.code(), cfg, Some(&mut hook), false)
        }
        None => crate::lower::lower(p.code(), cfg),
    }
    .ok_or(Fail::Permanent)?;
    #[cfg(not(feature = "trace"))]
    let low = crate::lower::lower(p.code(), cfg).ok_or(Fail::Permanent)?;
    // `on` mode skips functions the templates would barely help (always/verify compile everything)
    if d.heuristic && !low.worth_compiling() {
        return Err(Fail::Permanent);
    }
    let em = x86::emit(&low.prog).map_err(|_| Fail::Permanent)?;
    #[cfg(feature = "trace")]
    crate::tr::phase::add(crate::tr::phase::TIER1, t_tier1);
    if let Ok(dir) = std::env::var("RUSTYNAIL_JIT_DUMP") {
        // debugging aid: raw machine code per prototype (`objdump -D -b binary -mi386:x86-64`)
        let _ = std::fs::write(
            format!("{dir}/proto-{:x}.bin", p.code_ptr() as usize),
            &em.code,
        );
    }
    let Some(off0) = entry0_offset(&low, &em) else {
        return Err(Fail::Permanent);
    };
    // the trace table is charged next to the descriptor (refunded with the prototype)
    #[cfg(feature = "trace")]
    let tr_bytes = dis
        .as_ref()
        .map_or(0, crate::tr::cache::Dispatch::table_bytes);
    #[cfg(not(feature = "trace"))]
    let tr_bytes = 0usize;
    if tr_bytes > 0 && !jit_hooks::charge_code(st, tr_bytes) {
        d.deny = 32;
        return Err(Fail::Retry);
    }
    let Placed { block, own } = match place_code(st, dp, &em.code, 16) {
        Ok(p) => p,
        Err(e) => {
            if tr_bytes > 0 {
                jit_hooks::uncharge_code(st, tr_bytes);
            }
            return Err(e);
        }
    };
    // SAFETY: `dp` is still the live per-state data.
    let d = unsafe { &mut *dp };
    let resume = resume_table(p.code().len(), &low, &em);
    let _ = d
        .arena
        .register_symbol(&block, &format!("lua_proto@{:p}", p.code_ptr()));
    if let Some(pr) = d.prof.as_mut() {
        pr.protos.insert(
            p.code_ptr() as usize,
            ProtoProf {
                line: p.linedefined,
                total: low.total,
                templates: low.templates,
                loop_total: low.loop_total,
                loop_templates: low.loop_templates,
                entry_ok: low.entry_ok,
                interp_entry: low.interp_entry,
                code_bytes: em.code.len(),
                code: p.code().to_vec(),
                ..ProtoProf::default()
            },
        );
    }
    d.stats.compiled += 1;
    d.stats.code_bytes += em.code.len() as u64;
    let base = block.addr();
    let t15 = t15data.map(|data| Box::new(T15Proto::new(data, &low, &em)));
    let mut jp = Box::new(JitProto {
        hdr: JitProtoHdr {
            entry0: if low.entry_ok { base + off0 } else { 0 },
            base_m1: base - 1,
            resume: std::ptr::null(),
        },
        interp_entry: low.interp_entry,
        resume,
        code_ptr: p.code_ptr(),
        block,
        owner: dp,
        // the thunk's pages are not refunded with the proto (it lives as long as the state)
        charged: own + desc + tr_bytes,
        mon: Rc::new(Mon::default()),
        dead: std::cell::Cell::new(false),
        t15,
        #[cfg(feature = "trace")]
        tr: dis.and_then(|dis| dis.finish(base, &low, &em)),
    });
    jp.hdr.resume = jp.resume.as_ptr();
    // P-G: the recorder's landing structure comes from this lowering (no second lowering at the first recording)
    #[cfg(feature = "trace")]
    if let Some(tp) = jp.tr.as_deref() {
        // (debug builds, i.e. every test: the structure taken from this lowering is the one the recorder's own
        // lowering for the landing labels would compute)
        #[cfg(debug_assertions)]
        if let Some(fresh) = crate::tr::record::lower_for_landing(p) {
            assert_eq!(low.total, fresh.total, "landing cache: instruction count");
            for pc in 0..low.total as u32 {
                assert_eq!(
                    low.label_kind(pc),
                    fresh.label_kind(pc),
                    "landing cache: label kind at pc {pc}"
                );
                assert_eq!(low.rem(pc), fresh.rem(pc), "landing cache: rem at pc {pc}");
                assert_eq!(
                    low.cnt_at(pc),
                    fresh.cnt_at(pc),
                    "landing cache: cnt_at at pc {pc}"
                );
            }
        }
        tp.set_landing(low);
    }
    let raw = Box::into_raw(jp);
    d.live.insert(raw as usize);
    p.jit.set(raw.cast());
    Ok(())
}

/// Recompiles the prototype that owns descriptor `jpp` into a new code version (docs/JIT_TIER15.md 6.1):
/// `plans` are the promoted nests (placeholder for the region plans of T4). The old block is *retired*, not
/// freed: activations suspended in helpers called from it return into it, and it stays mapped until the
/// prototype is freed. On any failure the old version stays current and nothing observable changed.
///
/// The swap (`block`, `resume`, `hdr`, `pcmap`, the version tables) happens in one straight sequence after
/// everything that can fail succeeded; no generated code runs during a recompile (J-PACK), so no activation can
/// observe a half-swapped descriptor, whatever it was entered through (nested calls, resumed coroutines).
pub(crate) fn recompile(
    st: &mut State,
    p: &Proto,
    jpp: *mut JitProto,
    plans: &[NestPlan],
) -> Result<(), Fail> {
    recompile_with(st, p, jpp, plans, false)
}

/// [`recompile`]; a *give-up* recompile (the region of a nest that keeps failing is removed) may also use the
/// [`t15rt::GIVEUP_RESERVE`] compile slots beyond the normal cap.
pub(crate) fn recompile_with(
    st: &mut State,
    p: &Proto,
    jpp: *mut JitProto,
    plans: &[NestPlan],
    give_up: bool,
) -> Result<(), Fail> {
    let dp = data_ptr(st);
    // SAFETY: as in `set_mode`.
    let d = unsafe { &mut *dp };
    if !d.enabled {
        return Err(Fail::Permanent);
    }
    // SAFETY: `jpp` is the live descriptor of `p` (the caller checked `p.jit`); only this function and the
    // helpers that call it touch it while no generated code runs.
    let jp = unsafe { &mut *jpp };
    let Some(t15) = jp.t15.as_deref() else {
        return Err(Fail::Permanent);
    };
    let cap = t15rt::MAX_COMPILES + if give_up { t15rt::GIVEUP_RESERVE } else { 0 };
    if usize::from(t15.cur) + 1 >= cap {
        return Err(Fail::Permanent); // compile cap per prototype (tier 1 + 5, plus the give-up reserve)
    }
    // (`d.deny` is not consulted: the nest's own back-off paces the attempts)
    let ver = t15.cur + 1;
    let transient = transient_bytes(p);
    let vbytes = std::mem::size_of::<VersionData>();
    // MUTANT `vbytes_no_limit_guard`: a refused charge is ignored (the recompile goes on over the memory limit)
    if !jit_hooks::charge_code(st, transient + vbytes) && !vbytes_mutant("vbytes_no_limit_guard") {
        // SAFETY: as in `set_mode`.
        unsafe { (*dp).deny = 32 };
        return Err(Fail::Retry);
    }
    let r = recompile_inner(st, p, jpp, dp, plans, ver);
    jit_hooks::uncharge_code(st, transient);
    // MUTANT `vbytes_no_credit`: the version bytes of a failed recompile are not refunded
    if r.is_err() && !vbytes_mutant("vbytes_no_credit") {
        jit_hooks::uncharge_code(st, vbytes);
    }
    r
}

fn recompile_inner(
    st: &mut State,
    p: &Proto,
    jpp: *mut JitProto,
    dp: *mut JitData,
    plans: &[NestPlan],
    ver: u8,
) -> Result<(), Fail> {
    // SAFETY: see `recompile`.
    let (d, jp) = unsafe { (&mut *dp, &mut *jpp) };
    let env = call_env(st, d);
    let t15 = jp.t15.as_deref().expect("checked by recompile");
    // J5/F5 (TR19b): with traces on the new version gets the dispatch cells of the heads the regions do not own, the
    // dispatch table is the prototype's (`jp.tr`), and the version-install step below rewrites its cells.
    #[cfg(feature = "trace")]
    let mut dis = match jp.tr.as_deref() {
        Some(tp) if d.tr.cfg.mode != crate::tr::cache::TrMode::Off => {
            let ranges: Vec<(u32, u32)> = plans.iter().map(|p| (p.head, p.end)).collect();
            Some(
                crate::tr::cache::Dispatch::new(
                    crate::tr::cache::DispatchCfg {
                        hot: d.tr.cfg.hot_loop,
                        always: d.tr.cfg.mode == crate::tr::cache::TrMode::Always,
                        tracemove: d.tr.cfg.opts.tracemove,
                        seed: (p.linedefined as u32) ^ ((p.code().len() as u32) << 16),
                        line: p.linedefined,
                        fn_root: (d.tr.cfg.limits.fnroot != 0 || d.tr.cfg.limits.recnat != 0)
                            && !p.is_vararg(),
                        recnat: d.tr.cfg.limits.recnat != 0,
                        lazy: d.tr.cfg.limits.lazyci != 0,
                        fn_loops: d.tr.cfg.limits.recany != 0,
                        rec_any: d.tr.cfg.limits.recany != 0,
                        c_entered: d.tr.centry.contains(&(p.code_ptr() as usize)),
                    },
                    Some(tp),
                )
                .skipping(&ranges),
            )
        }
        _ => None,
    };
    #[cfg(feature = "trace")]
    let via_dispatch = dis.is_some();
    #[cfg(not(feature = "trace"))]
    let via_dispatch = false;
    let mut warm = t15.data.warmup(ver, plans);
    warm.via_dispatch = via_dispatch;
    let cfg = LowerCfg {
        code_addr: p.code_ptr() as usize,
        interrupt_addr: jit_hooks::interrupt_flag_addr(),
        calls: Some(env),
        consts: p.k(),
        t15_warmup: (!warm.nests.is_empty()).then_some(&warm),
    };
    let meta = {
        let mut escaped = crate::t15::SlotSet::EMPTY;
        for c in p.protos() {
            for u in c.upvalue_descs().iter().filter(|u| u.instack) {
                escaped.insert(u32::from(u.idx));
            }
        }
        t15rt::FuncMeta {
            max_stack: u32::from(p.maxstacksize),
            escaped,
            obs: t15
                .data
                .nests
                .iter()
                .map(|n| n.obs.borrow().to_vec())
                .collect(),
            prof: t15.data.prof.iter().map(std::cell::Cell::get).collect(),
            intr: t15.data.intr_sites(),
            opt: d.t15.opt.clone(),
        }
    };
    // policy inputs: widenings from exit feedback and the failure-pad addresses of every promoted nest
    let mut ctl = t15rt::PolicyCtl {
        nests: t15
            .data
            .nests
            .iter()
            .enumerate()
            .map(|(id, n)| t15rt::NestPolicy {
                entry_widen: n
                    .fb
                    .borrow()
                    .as_ref()
                    .map_or_else(Vec::new, |f| f.entry_widen.clone()),
                def_widen: n
                    .fb
                    .borrow()
                    .as_ref()
                    .map_or_else(Vec::new, |f| f.def_widen.clone()),
                fb: (!d.t15.opt.no_respec).then_some(t15rt::FeedbackSpec {
                    ver,
                    nest: id as u32,
                    fail_addr: n.fail.as_ptr() as usize,
                    ok_addr: n.entry_ok.as_ptr() as usize,
                    cont_addr: t15.data.cont.as_ptr() as usize,
                    event_fn: t15rt::jit_t15_event as *const () as usize as u64,
                }),
            })
            .collect(),
        sites: Vec::new(),
    };
    #[cfg(feature = "trace")]
    let low = {
        let mut hook;
        let extra: Option<&mut crate::lower::RegionHook<'_>> = match dis.as_mut() {
            Some(dis) => {
                hook = |cx: &mut crate::lower::Cx<'_>, env: &crate::lower::RegionEnv<'_>| {
                    crate::tr::cache::emit_dispatch(cx, env, dis)
                };
                Some(&mut hook)
            }
            None => None,
        };
        t15rt::lower_with_regions_hook(p.code(), cfg, plans, Some(&meta), Some(&mut ctl), extra)
    }
    .ok_or(Fail::Permanent)?;
    #[cfg(not(feature = "trace"))]
    let low = t15rt::lower_with_regions_ex(p.code(), cfg, plans, Some(&meta), Some(&mut ctl))
        .ok_or(Fail::Permanent)?;
    let regions = crate::t15::codegen::take_regions_emitted();
    // condition (3) of docs/JIT_TIER15.md section 12: every version has the same block structure, resume points
    // and function entry. A mismatch is a compiler bug: refuse the new version (the old one keeps running).
    let same = t15.data.sig.get() == Some(&low.sig);
    debug_assert!(same, "block structure differs between code versions");
    if !same {
        return Err(Fail::Permanent);
    }
    let em = x86::emit(&low.prog).map_err(|_| Fail::Permanent)?;
    if let Ok(dir) = std::env::var("RUSTYNAIL_JIT_DUMP") {
        let _ = std::fs::write(
            format!("{dir}/proto-{:x}-v{ver}.bin", p.code_ptr() as usize),
            &em.code,
        );
    }
    let Some(off0) = entry0_offset(&low, &em) else {
        return Err(Fail::Permanent);
    };
    let Placed { block, own } = place_code(st, dp, &em.code, 16)?;
    // SAFETY: as above (`place_code` touched the arena only).
    let (d, jp) = unsafe { (&mut *dp, &mut *jpp) };
    let resume = resume_table(p.code().len(), &low, &em);
    let pcmap = t15rt::pc_map(&low, &em);
    let _ = d
        .arena
        .register_symbol(&block, &format!("lua_proto@{:p}.v{ver}", p.code_ptr()));
    d.stats.code_bytes += em.code.len() as u64;
    d.stats.t15_recompiled += 1;
    d.stats.t15_regions += u64::from(regions);
    let base = block.addr();
    let entry_ok = low.entry_ok;
    let t15 = jp.t15.as_deref_mut().expect("checked by recompile");
    // ---- the swap: nothing below can fail or run generated code
    for (id, n) in t15.data.nests.iter().enumerate() {
        let mine: Vec<_> = ctl
            .sites
            .iter()
            .filter(|(nid, _)| *nid as usize == id)
            .map(|(_, s)| s.clone())
            .collect();
        if !mine.is_empty() || n.fb.borrow().is_some() {
            n.fb_mut().sites = mine;
        }
    }
    t15.versions.push(Box::new(VersionData::new(ver, 0)));
    t15.cur = ver;
    t15.pcmap = pcmap;
    t15.data.compiles.set(ver + 1);
    let old = std::mem::replace(&mut jp.block, block);
    #[cfg(feature = "trace")]
    let drop_old = crate::tr::cache::mutant_retired_freed_early();
    #[cfg(not(feature = "trace"))]
    let drop_old = false;
    if crate::t15::mutants::active("pol_drop_old_block") || drop_old {
        // MUTANT: activations suspended in the old block return into freed code
        // SAFETY: test-only mutant (cargo feature `t15_mutants`); it exists to be detected.
        let _ = unsafe { d.arena.free(old) };
    } else {
        t15.retired.push(old);
    }
    jp.resume = resume;
    jp.hdr = JitProtoHdr {
        entry0: if entry_ok { base + off0 } else { 0 },
        base_m1: base - 1,
        resume: jp.resume.as_ptr(),
    };
    jp.charged += own + std::mem::size_of::<VersionData>();
    // J5/F5 (TR19b(c)): the same step rewrites every non-trace dispatch cell of the prototype to the new version's labels
    // (`Traced` cells keep pointing at their traces; the traces' landings keep using the version recorded at compile time,
    // whose block is retired, not freed)
    #[cfg(feature = "trace")]
    if let Some(dis) = dis.as_ref() {
        dis.install_version(base, &low, &em);
        if let Some(tp) = jp.tr.as_deref() {
            tp.sync_all_nests();
        }
        crate::tr::cache::verify_landings(d);
    }
    Ok(())
}

fn hot_call(st: &mut State, ci: Ci, p: &Proto) -> HotCall {
    // BUNDLE-1 (2): a tail-called frame counts only with the `tailhook` knob (a plain call frame never has `CIST_TAIL` here)
    #[cfg(feature = "trace")]
    if ci.r().callstatus & rn_core::state::CIST_TAIL != 0 {
        // SAFETY: the per-state data is live (see `set_mode`); read-only.
        let th = unsafe { &*data_ptr(st) }.tr.cfg.limits.tailhook;
        let th = if crate::tr::cache::mutant_tailhook_ignored() {
            0
        } else {
            th
        };
        if th == 0 {
            p.hot.set(jit_hooks::initial_hot().saturating_mul(32));
            return HotCall::Skip;
        }
        if th >= 2 {
            // compile-only: a tail-called frame is never entered from the interpreter (compiled callers tail-call compiled callees inline)
            if p.jit.get().is_null() {
                let _ = ensure_compiled(st, p);
            }
            return HotCall::Skip;
        }
        // a tail-called frame that is refused for now (short stays) retries much later than a called one: the tail call of a
        // compiled callee is frequent (takl) and its cold path would run every few calls
        if p.jit.get() as usize > NOJIT && jit_hooks::TAIL_FROM_JIT.with(|c| c.get()) {
            p.hot.set(jit_hooks::initial_hot().saturating_mul(32));
            return HotCall::Skip;
        }
        let r = hot_call_plain(st, p, 0);
        return match r {
            HotCall::Retry => {
                p.hot.set(jit_hooks::initial_hot().saturating_mul(32));
                HotCall::Skip
            }
            r => r,
        };
    }
    // BUNDLE-J (`centry`): a Lua function that a C function calls (`table.sort` comparator, `string.gsub` callback, `pcall`, a metamethod called by a library: the caller's
    // frame is native) has no compiled caller to fall back on, so it is entered into compiled code although it is small and loop-free, and its entry head is a generic
    // function root (it is registered before its first compile, which is what decides whether the prototype gets a head without a `CALL`).
    // (`CIST_FRESH` is not yet set when the entry counter fires inside `pre_call`: the caller's frame is what tells.)
    #[cfg(feature = "trace")]
    let fresh = {
        // SAFETY: the per-state data is live (see `set_mode`); read-only.
        let dd = unsafe { &*data_ptr(st) };
        let centry = if dd.tr.cfg.mode == crate::tr::cache::TrMode::Off {
            0
        } else {
            dd.tr.cfg.limits.centry
        };
        if centry != 0 && {
            let prev = ci.r().previous;
            // SAFETY: `previous` of a live frame is a live frame (the base frame has no previous: it is native).
            prev.is_null() || unsafe { (*prev).callstatus } & rn_core::state::CIST_C != 0
        } {
            if p.jit.get().is_null() && !p.is_vararg() {
                // SAFETY: as above; mutated before any call that could reach the data again.
                let d = unsafe { &mut *data_ptr(st) };
                if d.tr.centry.insert(p.code_ptr() as usize) {
                    d.tr.stats.c_entered += 1;
                }
            }
            centry
        } else {
            0
        }
    };
    #[cfg(not(feature = "trace"))]
    let fresh = {
        let _ = ci;
        0u32
    };
    #[cfg(feature = "trace")]
    if fresh != 0 {
        // BUNDLE-J (`centrywait`): the function is compiled (with its head) at its first hot event from C, but entered only from the `centrywait`-th on
        let r = hot_call_plain(st, p, fresh);
        // SAFETY: the per-state data is live (see `set_mode`).
        let d = unsafe { &mut *data_ptr(st) };
        if matches!(r, HotCall::Enter) && d.tr.centry.contains(&(p.code_ptr() as usize)) {
            let n = d.tr.centry_seen.entry(p.code_ptr() as usize).or_insert(0);
            if *n < d.tr.cfg.limits.centrywait && !crate::tr::mutants::active("centry_wait_ignored")
            {
                *n += 1;
                return HotCall::Retry;
            }
        }
        return r;
    }
    hot_call_plain(st, p, fresh)
}

fn hot_call_plain(st: &mut State, p: &Proto, fresh: u32) -> HotCall {
    if ensure_compiled(st, p) {
        // SAFETY: a compiled proto's `jit` is a live descriptor (see `hot_loop`).
        let jp = unsafe { &*(p.jit.get() as *const JitProto) };
        #[cfg(feature = "trace")]
        let light = fresh != 0 && jp.tr.as_deref().is_some_and(|tp| tp.cent_off());
        #[cfg(not(feature = "trace"))]
        let light = false;
        if light || jp.hdr.entry0 == 0 || (!jp.interp_entry && fresh == 0 && heuristic(st)) {
            // compiled for its loops only, or too small to pay for the round trip from the interpreter
            // (compiled callers still flat-call it)
            HotCall::NoJit
        } else if fresh < 2
            && monitoring(st)
            && jp.mon().blocked(jit_hooks::DEFAULT_ENTRY_CALLS as u32)
        {
            HotCall::Retry // short stays: the interpreter runs it for now (back-off)
        } else {
            #[cfg(feature = "trace")]
            if fresh != 0 {
                // SAFETY: the per-state data is live (see `set_mode`).
                unsafe { (*data_ptr(st)).tr.stats.c_enters += 1 };
            }
            HotCall::Enter
        }
    } else if p.jit.get() as usize == NOJIT {
        HotCall::NoJit
    } else {
        HotCall::Retry
    }
}

/// Native offset of the entry for `pc` in `jp`.
fn entry_offset(jp: &JitProto, pc: *const u32) -> Option<u32> {
    let idx = (pc as usize).checked_sub(jp.code_ptr as usize)? / 4;
    let off = *jp.resume.get(idx)?;
    (off != 0 && off & crate::lower::calls::RESUME_MM_ONLY == 0).then(|| off - 1)
}

/// R1: the knob `rearm` counts back-edges of the frame's own loops; the tick's span is kept in units of 1/`REARM_UNIT` of one (loops of callees cost one unit).
const REARM_UNIT: u32 = 32;
/// R1: Lua frames above the stay's entry frame that the arming looks through.
const REARM_DEPTH: usize = 8;

#[cfg(feature = "trace")]
/// R1 / BUNDLE-P / C45: may a rearmed tick enter a head whose root trace `trace` has `entry_ok` entries? The family test (a scan-free lookup) only runs for a young head.
fn rearm_entry_ok(d: &mut JitData, entry_ok: u32, trace: Option<crate::tr::cache::TrId>) -> bool {
    if entry_ok >= crate::tr::cache::REARM_MIN_ENTRIES {
        return true;
    }
    let (nside, faults, la) = trace.map_or((0, true, true), |id| d.tr.family_clean(id));
    let l = &d.tr.cfg.limits;
    let ok = crate::tr::cache::rearm_young_ok(
        entry_ok,
        nside,
        !faults && !la,
        l.rearmside,
        l.rearmclean,
    );
    if !ok && l.rearmclean != 0 && l.rearmside != 0 && nside >= l.rearmside {
        // refused by the clean test alone
        d.tr.stats.rearm_unclean[0] += u64::from(faults);
        d.tr.stats.rearm_unclean[1] += u64::from(la);
    }
    ok
}

/// R1 (`rearm`): a rearmed tick (armed by [`rearm_after_exit`]) fired at the back-edge of the interpreter frame `ci` (next instruction `pc`).
/// It enters tier 1 only at a loop head that holds an installed root trace, and leaves everything else alone (no compile, no start-gate clock, no
/// `monwarm` credit, no entry-monitor query, the head's counter and state untouched). A head whose rearm stays are all short is refused
/// (budget per head: [`crate::tr::cache::REARM_BUDGET`]). A miss re-arms the tick for the next back-edge while the span lasts.
#[cfg(feature = "trace")]
fn rearm_loop(st: &mut State, ci: Ci, pc: *const u32, span: u32) -> bool {
    use crate::tr::cache::{HeadState, REARM_BUDGET, REARM_SHORT, REARM_WAIT};
    // SAFETY: the per-state data is live (see `set_mode`); the borrow ends before any call that could reach it again.
    let d = unsafe { &mut *data_ptr(st) };
    if jit_hooks::tick_left(st) < jit_hooks::tick_period() {
        d.tr.stats.rearm_restored += 1;
    }
    if d.tr.cfg.limits.rearm == 0
        || d.tr.cfg.mode == crate::tr::cache::TrMode::Off
        || crate::tr::mutants::active("rearm_never")
    {
        return false;
    }
    let key = proto_of(st, ci).map_or(0, |cl| cl.proto().code_ptr() as usize);
    let hit = (|| {
        let cl = proto_of(st, ci)?;
        let p = cl.proto();
        let jp = p.jit.get() as usize;
        if jp <= NOJIT {
            return None;
        }
        // SAFETY: as in `hot_loop`.
        let jp = unsafe { &*(jp as *const JitProto) };
        entry_offset(jp, pc)?;
        let idx = ((pc as usize) - (jp.code_ptr as usize)) / 4;
        let tp = jp.tr.as_ref()?;
        let h = tp.head(tp.head_index(idx as u32)?);
        if !crate::tr::mutants::active("rearm_any_head")
            && (h.state.get() != HeadState::Traced
                || h.trace.get().is_none()
                || !rearm_entry_ok(&mut *d, h.entry_ok.get(), h.trace.get()))
        {
            return None;
        }
        let ins = d.stats.insns;
        if h.rearm_budget.get() == 0 {
            let w = h.rearm_wait.get().saturating_add(1);
            if w < REARM_WAIT {
                h.rearm_wait.set(w);
                return None;
            }
            h.rearm_wait.set(0);
            h.rearm_budget.set(1);
        } else if h.rearm_last.get() != 0 {
            if ins.saturating_sub(h.rearm_last.get()) < REARM_SHORT {
                h.rearm_budget.set(h.rearm_budget.get() - 1);
            } else {
                h.rearm_budget.set(REARM_BUDGET);
            }
        }
        h.rearm_last.set(ins.max(1));
        Some(())
    })();
    if hit.is_some() {
        d.tr.stats.rearm_enters += 1;
        d.tr.rearm_entering = true;
        return true;
    }
    d.tr.stats.rearm_misses += 1;
    // a back-edge of another prototype's loop (a callee of the frame the arming was made for) costs a unit, one of the target's own loops [`REARM_UNIT`]
    let cost = if key == jit_hooks::rearm_key(st) {
        REARM_UNIT
    } else {
        1
    };
    if span > cost {
        jit_hooks::rearm_tick(st, span - cost, jit_hooks::rearm_key(st));
    }
    false
}

/// R1 (`rearm`): a stay of compiled code ended in the interpreter (`code` 0: in the frame `ret_ci`; 1: in `st.ci()`). When that frame, or a Lua caller of
/// it below no C boundary, belongs to a prototype with an installed root trace, arms the rearmed tick.
#[cfg(feature = "trace")]
fn rearm_after_exit(st: &mut State, code: u32, ret_ci: usize, entry: (usize, usize)) {
    // SAFETY: the per-state data is live (see `set_mode`); read-only.
    let (span, on) = {
        let d = unsafe { &*data_ptr(st) };
        (
            d.tr.cfg.limits.rearm,
            d.tr.cfg.mode != crate::tr::cache::TrMode::Off,
        )
    };
    if span == 0 || !on || st.hook_mask() != 0 || crate::tr::mutants::active("rearm_never") {
        return;
    }
    let mut c = if code == 0 {
        // SAFETY: on `Continue` `ret.ci` is the live frame compiled code stopped in (`r13`).
        unsafe { Ci::from_raw(ret_ci as *mut _) }
    } else {
        st.ci()
    };
    for _ in 0..REARM_DEPTH {
        if !c.r().is_lua_code() {
            return;
        }
        // only the frame the stay was entered at (or a frame on top of it) hands over: a stay that was entered at a function and popped it
        // (return into the interpreter's frame below) leaves loops of the callers to the sampling tick
        if (c.as_ptr() as usize, c.r().func.0) != entry {
            match c.previous() {
                Some(n) => c = n,
                None => return,
            }
            continue;
        }
        if let Some(cl) = proto_of(st, c) {
            let p = cl.proto();
            let jp = p.jit.get() as usize;
            if jp > NOJIT {
                // SAFETY: as in `hot_loop`.
                let jp = unsafe { &*(jp as *const JitProto) };
                if jp.tr.as_ref().is_some_and(|tp| tp.any_traced()) {
                    // SAFETY: as above.
                    unsafe { &mut *data_ptr(st) }.tr.stats.rearm_arms += 1;
                    jit_hooks::rearm_tick(st, span * REARM_UNIT, p.code_ptr() as usize);
                    return;
                }
            }
        }
        match c.previous() {
            Some(n) => c = n,
            None => return,
        }
    }
}

fn hot_loop(st: &mut State, ci: Ci, pc: *const u32, rearmed: u32) -> bool {
    #[cfg(feature = "trace")]
    if rearmed != 0 {
        if crate::tr::mutants::active("rearm_tick_reset") {
            jit_hooks::tick_reset(st);
        }
        if !crate::tr::mutants::active("rearm_warm_credit") {
            return rearm_loop(st, ci, pc, rearmed);
        }
        // (mutant: the rearmed tick also feeds the start-gate clock and the monitor)
    }
    #[cfg(not(feature = "trace"))]
    let _ = rearmed;
    // P-G: the first sampling tick of the interpreter's loops starts the clock of the recorder's start gate
    #[cfg(feature = "trace")]
    {
        // SAFETY: the per-state data is live (see `set_mode`).
        let len = proto_of(st, ci).map_or(8, |cl| {
            crate::tr::cache::loop_len(
                cl.proto().code(),
                ((pc as usize).wrapping_sub(cl.proto().code_ptr() as usize)) / 4,
            )
        });
        unsafe { &mut *data_ptr(st) }.tr.warm_touch(len);
    }
    let Some(cl) = proto_of(st, ci) else {
        return false;
    };
    let p = cl.proto();
    if !ensure_compiled(st, &p) {
        return false;
    }
    let jp = p.jit.get() as usize;
    if jp <= NOJIT {
        return false;
    }
    // SAFETY: `Proto.jit` of a compiled proto points to a `JitProto` owned by this state's `JitData`
    // (a proto belongs to exactly one state); the descriptor is alive as long as the *prototype* is
    // (`free_code` runs when the collector frees it), and the caller holds a closure of it.
    let jp = unsafe { &*(jp as *const JitProto) };
    if entry_offset(jp, pc).is_none() {
        return false;
    }
    let blocked = monitoring(st) && jp.mon().blocked(1);
    // R1: a loop with a call in its body (checked by `loop_calls`: a C function or a callee that is not compiled leaves tier 1
    // at every iteration, so the J4 stay monitor above sees one-iteration stays and refuses entries) has its trace hotness
    // counted in compiled code only, one back-edge per entry from a sampling tick: 256 ticks (256 000 interpreted iterations)
    // before the first recording. While the loop head has no trace and no failed recording, the tick therefore enters even when
    // the back-off refuses, and credits an eighth of the head's threshold (`TrProto::tick_credit`: a recording after eight ticks,
    // 8000 interpreted iterations; a short-lived loop never reaches it and costs no recording) to the head's counter (`monwarm`).
    // (Loops without calls are left alone: they count their own back-edges, and the start time of heapsort's `sift` recordings
    // decides whether it gets a trace at all.)
    #[cfg(feature = "trace")]
    {
        // SAFETY: the per-state data is live (see `set_mode`); read-only.
        let d = unsafe { &*data_ptr(st) };
        if d.tr.cfg.mode != crate::tr::cache::TrMode::Off {
            let idx = ((pc as usize) - (jp.code_ptr as usize)) / 4;
            let calls = loop_calls(p.code(), idx);
            let warm = jp.tr.as_ref().filter(|_| calls).and_then(|tp| {
                let h = tp.head(tp.head_index(idx as u32)?);
                (h.state.get() == crate::tr::cache::HeadState::Warm)
                    .then_some((h, tp.tick_credit()))
            });
            if let Some((h, credit)) = warm {
                // (once per head: a head that never failed a recording. The tick is phase-locked to loops whose trip count is a
                // multiple of the period (every tick lands in the same iteration, often the last one: `LeaveLoop`), and stays
                // that end after one iteration feed the entry monitor, which then refuses the function entries too)
                if h.fresh() {
                    h.credit(credit, 0);
                    return true;
                }
            }
        }
    }
    !blocked
}

/// R1: does the loop whose head is `head` contain a call (`CALL` / `TAILCALL`) between the head and its backward edge? Such a loop
/// leaves compiled tier-1 code at every iteration (a C function or a not yet compiled callee), so its entry back-off says
/// nothing about the iteration count. (No backward edge to `head` found: no.)
fn loop_calls(code: &[u32], head: usize) -> bool {
    use rn_core::instruction::Instruction;
    use rn_core::ver::active::OpCode as O;
    let mut call = false;
    for (j, &w) in code.iter().enumerate().skip(head) {
        let i = Instruction(w);
        match i.opcode() {
            Some(O::Call | O::Tailcall) => call = true,
            Some(O::Jmp) if j as i64 + 1 + i64::from(i.sj()) == head as i64 => return call,
            Some(O::Forloop | O::Tforloop) if j as i64 + 1 - i64::from(i.bx()) == head as i64 => {
                return call
            }
            _ => {}
        }
    }
    false
}

/// Entry back-off is active in `on` mode only (`always`/`verify` stay deterministic).
fn monitoring(st: &mut State) -> bool {
    heuristic(st)
}

fn heuristic(st: &mut State) -> bool {
    // SAFETY: the per-state data is live (see `set_mode`).
    unsafe { &*data_ptr(st) }.heuristic
}

/// Stays entered by any state of this process (test oracle only: tests/tr_identity_pre.rs must know whether
/// compiled code ran between two interpreter fetches).
#[cfg(feature = "t15_oracle")]
pub static ENTERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Test-only stay accounting (feature `t15_oracle`; tests/tr_identity.rs, tests/tr_oracle.rs: the TR5.6 no-overshoot
/// check and the oracle clock). Process-wide, like [`ENTERS`]; tests that read it run one state at a time.
#[cfg(feature = "t15_oracle")]
pub mod stay_stats {
    use std::sync::atomic::{AtomicI64, AtomicU64};
    /// Stays in progress (a nested stay, entered by a helper of an outer one, counts too).
    pub static OPEN: AtomicI64 = AtomicI64::new(0);
    /// Stays that finished.
    pub static ENDED: AtomicU64 = AtomicU64::new(0);
    /// Sum of the instructions the finished stays executed (= the sum of `Stats::insns` over the states).
    pub static INSNS: AtomicU64 = AtomicU64::new(0);
    /// Stays that ended with a negative budget (executed more than they were entered with).
    pub static OVERSHOOTS: AtomicU64 = AtomicU64::new(0);
    /// The largest overshoot seen.
    pub static MAX_OVER: AtomicU64 = AtomicU64::new(0);
    /// Instructions the suspended outer stays had consumed when the stays above them were entered (a nested entry
    /// saves the outer counter: `budget - saved counter`, prepaid block remainder included).
    pub static SUSPENDED: AtomicI64 = AtomicI64::new(0);

    thread_local! {
        /// `(budget, consumed-at-suspension)` of every open stay of this thread, outermost first.
        static STACK: std::cell::RefCell<Vec<(i64, i64)>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// The counter word (`r15`) of the state whose stay is innermost.
    static CNT: std::sync::atomic::AtomicPtr<i32> =
        std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

    /// Instructions consumed by the stays that are open (suspended in a helper, or running) at this moment, for a
    /// caller that is NOT inside compiled code: the interpreter running a callee of compiled code. The innermost
    /// stay's consumption is `budget - counter word` (a stay that is itself suspended inside a nested stay is in
    /// [`SUSPENDED`]). Includes the prepaid remainder of the block the stay is suspended in.
    pub fn consumed_while_suspended() -> i64 {
        use std::sync::atomic::Ordering::Relaxed;
        let open = OPEN.load(Relaxed);
        let mut c = SUSPENDED.load(Relaxed);
        if open > 0 {
            let p = CNT.load(Relaxed);
            let b = STACK.with(|s| s.borrow().last().map_or(0, |o| o.0));
            // SAFETY: `begin` stored the live per-state counter word of the innermost open stay.
            c += b - i64::from(unsafe { p.read_volatile() });
        }
        c
    }

    /// A stay with budget `budget` starts; `saved` is the counter word of the stay it interrupts (if any), `cnt` the
    /// word compiled code counts in.
    pub(crate) fn begin(budget: i32, saved: i32, cnt: *mut i32) {
        use std::sync::atomic::Ordering::Relaxed;
        CNT.store(cnt, Relaxed);
        STACK.with(|s| {
            let mut s = s.borrow_mut();
            let partial = s.last().map_or(0, |o| o.0 - i64::from(saved));
            if let Some(o) = s.last_mut() {
                o.1 = partial;
                SUSPENDED.fetch_add(partial, Relaxed);
            }
            s.push((i64::from(budget), 0));
            OPEN.store(s.len() as i64, Relaxed);
        });
    }

    /// The innermost stay ends: the stay below it resumes.
    pub(crate) fn end() {
        use std::sync::atomic::Ordering::Relaxed;
        STACK.with(|s| {
            let mut s = s.borrow_mut();
            s.pop();
            if let Some(o) = s.last_mut() {
                SUSPENDED.fetch_sub(o.1, Relaxed);
                o.1 = 0;
            }
            OPEN.store(s.len() as i64, Relaxed);
        });
    }
}

fn enter(st: &mut State, ci: Ci) -> JitExit {
    // SAFETY: the per-state data is live (see `set_mode`).
    if !unsafe { &*data_ptr(st) }.enabled {
        return JitExit::Continue; // `set_mode(Off)`: already compiled protos are not entered any more
    }
    // TR18: flushes queued while an exit handler ran are executed at the next safe point
    #[cfg(feature = "trace")]
    // SAFETY: the per-state data is live (see `set_mode`); read-only test.
    if unsafe { &*data_ptr(st) }.tr.has_deferred() {
        crate::tr::cache::safe_point(st);
    }
    let Some(cl) = proto_of(st, ci) else {
        return JitExit::Continue;
    };
    let p = cl.proto();
    let jp = p.jit.get() as usize;
    if jp <= NOJIT {
        return JitExit::Continue;
    }
    // SAFETY: as in `hot_loop`; the reference is not used after the call of compiled code below.
    let jp = unsafe { &*(jp as *const JitProto) };
    let Some(off) = entry_offset(jp, ci.r().savedpc) else {
        return JitExit::Continue;
    };
    let target = jp.block.addr() + off as usize;
    let at_start = ci.r().savedpc == jp.code_ptr;
    // R1: the frame this stay is entered at (frame node and function slot; `ci` itself is not touched after the call)
    let entry_key = (ci.as_ptr() as usize, ci.r().func.0);
    // Everything needed after the call is taken here, by value or shared ownership: while compiled code runs,
    // this frame can return flat into a compiled caller, which can run the collector and free `p` (its
    // `JitProto` and the prototype itself). Neither `p`, `jp` nor `ci` may be dereferenced afterwards.
    let mon = Rc::clone(&jp.mon);
    let proto_key = p.code_ptr() as usize;
    let dp = data_ptr(st);
    // No `&mut JitData` is kept across the call: compiled code calls helpers, which compile (mutating the
    // per-state data) and can run the collector (which frees code through `free_code`).
    // SAFETY: the per-state data is live (see `set_mode`); the borrow ends in this block.
    #[cfg(feature = "trace")]
    // SAFETY: the per-state data is live (see `set_mode`).
    let rearm_entry = std::mem::take(&mut unsafe { &mut *data_ptr(st) }.tr.rearm_entering);
    #[cfg(not(feature = "trace"))]
    let rearm_entry = false;
    let (thunk, cnt, budget, saved_cnt) = unsafe {
        let d = &mut *dp;
        let Some((thunk, _)) = &d.thunk else {
            return JitExit::Continue;
        };
        let thunk = *thunk;
        // `cnt` is the one counter word every activation's `r15` points at: a nested entry (a metamethod,
        // `pcall`, a sort comparator, an iterator, a resumed coroutine ...) runs while an outer activation is
        // suspended in a helper. Save the outer value and put it back afterwards, so the outer count is
        // neither reset nor charged for the nested stay.
        let saved_cnt = d.hdr.cnt;
        let stay = d.budget;
        d.hdr.cnt = stay;
        d.stats.enters += 1;
        #[cfg(feature = "t15_oracle")]
        ENTERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "t15_oracle")]
        stay_stats::begin(stay, saved_cnt, (&raw mut d.hdr).cast::<i32>());
        if !at_start {
            d.stats.osr_enters += 1;
        }
        (thunk, (&raw mut d.hdr).cast::<i32>(), stay, saved_cnt)
    };
    let base = st.slot_ptr(StkId(ci.r().func.0 + 1));
    let thr = jit_hooks::thread_ptr(st);
    // SAFETY: the per-state data is live (see `set_mode`); read-only test.
    let t0 = if unsafe { (*dp).prof.is_some() } {
        rdtsc()
    } else {
        0
    };
    // SAFETY: `thunk` is the committed entry thunk of this state's arena; `thr`, `ci` and `base` are the
    // live thread, frame and register base of the running Lua function; `target` is a resume address of
    // the proto's committed code block; `cnt` outlives the call (boxed per-state data). Compiled code
    // follows the register/ABI contract of `x86`, calls only the `extern "C"` helpers of
    // `rn_core::jit_helpers` and returns through the thunk's epilogue.
    let ret = unsafe {
        thunk(
            thr,
            ci.as_ptr().cast(),
            base.cast(),
            cnt,
            target as *const u8,
        )
    };
    let code = ret.code;
    // SAFETY: as above; re-derived after the call.
    let d = unsafe { &mut *dp };
    let left = d.hdr.cnt;
    d.hdr.cnt = saved_cnt;
    let executed = (i64::from(budget) - i64::from(left)).max(0) as u64;
    #[cfg(feature = "t15_oracle")]
    {
        use std::sync::atomic::Ordering::Relaxed;
        stay_stats::end();
        stay_stats::ENDED.fetch_add(1, Relaxed);
        stay_stats::INSNS.fetch_add(executed, Relaxed);
        if left < 0 {
            stay_stats::OVERSHOOTS.fetch_add(1, Relaxed);
            stay_stats::MAX_OVER.fetch_max(u64::from(left.unsigned_abs()), Relaxed);
        }
    }
    d.stats.insns += executed;
    if d.heuristic && !rearm_entry {
        mon.stay(executed);
    }
    if t0 != 0 && (code == 0 || code == 1) {
        let cyc = rdtsc().wrapping_sub(t0);
        // the frame compiled code left in (`ci` itself may have been popped and its slot reused)
        let fin = if code == 0 {
            // SAFETY: on `Continue` `ret.ci` is the live frame compiled code stopped in (`r13`).
            unsafe { Ci::from_raw(ret.ci as *mut _) }
        } else {
            st.ci()
        };
        if let Some(cl) = proto_of(st, fin) {
            let key = cl.proto().code_ptr() as usize;
            let idx = (fin.r().savedpc as usize).wrapping_sub(key) / 4;
            if let Some(pr) = d.prof.as_mut() {
                let e = pr.exits.entry((key, idx as u32)).or_default();
                e.0 += 1;
                e.1 += cyc;
                if let Some(pp) = pr.protos.get_mut(&proto_key) {
                    pp.enters += 1;
                    pp.cycles += cyc;
                    pp.insns += executed;
                    pp.blocks = mon.blocks.get();
                }
            }
        }
    }
    match code {
        0 => {
            // compiled code may have switched frames (flat calls / returns): the interpreter continues
            // with the frame compiled code left in (`r13`), not with the frame it entered. (`thread.ci`
            // is not used: `coroutine.close` of the running thread resets it behind the frame's back.)
            if ret.ci != ci.as_ptr() as usize {
                d.stats.exit_frame_changed += 1;
                #[cfg(feature = "trace")]
                rearm_after_exit(st, 0, ret.ci, entry_key);
                return JitExit::FrameChanged;
            }
            d.stats.exit_continue += 1;
            // N3b: a stay that ran a trace for a while ended in the interpreter (an allocation exit, the metamethod behind an
            // instruction tier 1 cannot do): the loop wants back in compiled code at its next back-edge, not after a whole tick period
            #[cfg(feature = "trace")]
            if jit_hooks::take_alloc_refusal() && executed >= LONG_STAY {
                jit_hooks::arm_tick_next(st);
            }
            #[cfg(feature = "trace")]
            rearm_after_exit(st, 0, ret.ci, entry_key);
            JitExit::Continue
        }
        3 => JitExit::Error,
        1 => {
            d.stats.exit_frame_changed += 1;
            #[cfg(feature = "trace")]
            rearm_after_exit(st, 1, 0, entry_key);
            JitExit::FrameChanged
        }
        2 => {
            d.stats.exit_returned += 1;
            JitExit::Returned
        }
        _ => std::process::abort(), // corrupted exit code: P1 forbids unwinding through generated code
    }
}

fn rdtsc() -> u64 {
    // SAFETY: rdtsc has no preconditions on x86-64.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(test)]
mod mon_tests {
    use super::*;

    #[test]
    fn short_stays_block_with_doubling_backoff_and_long_stays_do_not() {
        let m = Mon::default();
        for _ in 0..MON_WINDOW {
            m.stay(10);
        }
        assert!(m.blocked(1), "a window of 10-bytecode stays blocks entries");
        for _ in 1..FIRST_SKIP {
            assert!(m.blocked(1));
        }
        assert!(!m.blocked(1), "unblocked after the skip budget");
        for _ in 0..MON_WINDOW {
            m.stay(10);
        }
        // second block is twice as long
        let mut n = 0;
        while m.blocked(1) {
            n += 1;
        }
        assert_eq!(n, 2 * FIRST_SKIP);
        // a profitable window resets the back-off
        for _ in 0..MON_WINDOW {
            m.stay(1000);
        }
        assert!(!m.blocked(1));
        assert_eq!(m.next_skip.get(), 0);
    }
}

/// Mutant switch of the recompile accounting; the registry lives in the trace module, so without the `trace`
/// feature no mutant is ever active.
#[inline]
fn vbytes_mutant(name: &str) -> bool {
    #[cfg(feature = "trace")]
    {
        crate::tr::mutants::active(name)
    }
    #[cfg(not(feature = "trace"))]
    {
        let _ = name;
        false
    }
}
