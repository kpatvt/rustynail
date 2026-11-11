//! Tier 1.5 runtime plumbing (docs/JIT_TIER15.md 6, review amendments B3 B4 M7 M9 A5; task T3). No typed region
//! is generated yet: a *promotion* recompiles the prototype into a new code version that is tier 1 again (same
//! labels, same block structure), but all of the machinery a region needs is live and tested:
//!
//! * [`T15Data`]: address-stable per-prototype state (promote counters per nest, `prof[pc]` last-tag bytes). The
//!   warm-up code of tier 1 embeds raw addresses into it, so it is a `Box` that is never moved, never grown and
//!   freed only together with the descriptor *and every code version that can still run* (B4).
//! * warm-up instrumentation (`LowerCfg::t15_warmup`, emitted by `lower.rs`): a promote counter on every back-edge
//!   of a nest, executed **before** the loop head's `CountSub` (M7), and a one-byte store of the result tag after
//!   the loads inside warm nests.
//! * [`jit_t15_promote`] / [`jit_t15_event`]: the helpers compiled code calls. Both carry the *version id* of the
//!   code they are called from. A promotion is idempotent (an activation still running in an old version that
//!   reaches the back-edge of an already promoted nest is moved to the current code), and the event helper reads
//!   site data of the version it is told, never of a newer version with an older id.
//! * the recompile itself lives in `rt.rs` (`recompile`): the old block is retired (kept mapped until the
//!   prototype is freed), `hdr`/`resume`/`pcmap` are swapped in one straight sequence, charges follow the code.
//! * the knobs `RUSTYNAIL_JIT_T15=off|on|always`, `RUSTYNAIL_JIT_T15_HOT`, `RUSTYNAIL_JIT_OPT` (parsing only).
//!
//! Continuations: the helpers return 0 ("continue in the code you are in") or 1 (the absolute continuation
//! address is in [`T15Data::cont`]; the caller does `JmpR`). The address cannot be returned directly because
//! `CallH` results are 32 bits wide.

use crate::lower::{lower_ex, Cx, LowerCfg, Lowered, RegionEnv, RegionOut};
use crate::rt::{self, Fail, JitProto, NOJIT};
use crate::x86::Emitted;
use rn_core::func::Proto;
use rn_core::instruction::Instruction;
use rn_core::jit_hooks;
use rn_core::state::{CallInfo, Ci, State, Thread};
use rn_core::value::Gc;
use rn_core::ver::active::OpCode;
use std::cell::{Cell, OnceCell, RefCell};

/// Back-edges per nest before it is promoted (`RUSTYNAIL_JIT_T15_HOT`).
pub const T15_HOT_DEFAULT: i32 = 4096;
/// Compiles per prototype: tier 1 plus 5 recompiles (docs/JIT_TIER15.md 3.8).
pub const MAX_COMPILES: usize = 6;
/// Extra compile slots that only a *give-up* recompile may use (a region that keeps failing is removed even when the
/// normal compile budget is spent; docs/JIT_TIER15.md 21).
pub const GIVEUP_RESERVE: usize = 2;
/// Respecialisations of one nest before the next trigger gives the region up (`gen` cap, docs 3.8).
pub const GEN_CAP: u8 = 3;
/// Default number of stub failures (non-peel) of a nest that trigger a policy decision (`EXIT_HOT`, doubled per
/// respecialisation).
pub const EXIT_HOT_DEFAULT: i32 = 256;
/// Back-off exponent limit of refused promotions (`hot << k` back-edges until the retry, k <= 10, then `Off`).
pub const MAX_BACKOFF: u8 = 10;

// ---- knobs -----------------------------------------------------------------------------------------------

/// `RUSTYNAIL_JIT_T15`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T15Mode {
    /// No tier 1.5 (default): the tier-1 code is exactly what it was before tier 1.5 existed.
    Off,
    /// Promote hot nests (threshold [`T15Config::hot`]).
    On,
    /// Promote every nest at its first back-edge (threshold 1; deterministic tests).
    Always,
}

/// `RUSTYNAIL_JIT_OPT`: comma list of pass switches (`-name` disables, `+name[=v]` enables). Only parsed in T3;
/// the passes that consult them come with T4-T8.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OptFlags {
    /// `-ra`: no registers at all.
    pub no_ra: bool,
    /// `-xmm`: floats in GPRs/slots.
    pub no_xmm: bool,
    /// `-hoist`.
    pub no_hoist: bool,
    /// `-fcache`.
    pub no_fcache: bool,
    /// `-intr`.
    pub no_intr: bool,
    /// `-cntreg`.
    pub no_cntreg: bool,
    /// `-calls`: no regions containing Call-class ops.
    pub no_calls: bool,
    /// `-nest`: innermost loop only.
    pub no_nest: bool,
    /// `-peel`: count peel failures as failures.
    pub no_peel: bool,
    /// `-tables`: table accesses stay fallbacks (no inline array/hash paths, T5).
    pub no_tables: bool,
    /// `+randra=SEED`.
    pub randra: Option<u64>,
    /// `+exitstorm=N`.
    pub exitstorm: Option<u32>,
    /// `+flushall`.
    pub flushall: bool,
    /// `+verify`.
    pub verify: bool,
    /// `+shadow`: shadow verification of side-effect-free region blocks (`t15::verify`).
    pub shadow: bool,
    /// `+force`: generate regions even when the profitability gate refuses them (tests).
    pub force: bool,
    /// `-respec`: no failure counters in stubs, no respecialisation (regions are never revised).
    pub no_respec: bool,
    /// `+exithot=N`: stub failures per policy decision (default [`EXIT_HOT_DEFAULT`]).
    pub exit_hot: Option<i32>,
    /// Items that are not a known switch (reported by VERBOSE; never an error).
    pub unknown: Vec<String>,
}

impl OptFlags {
    /// Parses a `RUSTYNAIL_JIT_OPT` value.
    pub fn parse(s: &str) -> OptFlags {
        let mut o = OptFlags::default();
        for item in s.split(',').map(str::trim).filter(|x| !x.is_empty()) {
            let (sign, rest) = item.split_at(1);
            let (name, val) = match rest.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (rest, None),
            };
            match (sign, name, val) {
                ("-", "ra", None) => o.no_ra = true,
                ("-", "xmm", None) => o.no_xmm = true,
                ("-", "hoist", None) => o.no_hoist = true,
                ("-", "fcache", None) => o.no_fcache = true,
                ("-", "intr", None) => o.no_intr = true,
                ("-", "cntreg", None) => o.no_cntreg = true,
                ("-", "calls", None) => o.no_calls = true,
                ("-", "nest", None) => o.no_nest = true,
                ("-", "peel", None) => o.no_peel = true,
                ("-", "tables", None) => o.no_tables = true,
                ("-", "respec", None) => o.no_respec = true,
                ("+", "exithot", Some(v)) => match v.parse::<i32>() {
                    Ok(x) if x >= 1 => o.exit_hot = Some(x),
                    _ => o.unknown.push(item.to_string()),
                },
                ("+", "flushall", None) => o.flushall = true,
                ("+", "verify", None) => o.verify = true,
                ("+", "shadow", None) => o.shadow = true,
                ("+", "force", None) => o.force = true,
                ("+", "randra", Some(v)) => match v.parse() {
                    Ok(x) => o.randra = Some(x),
                    Err(_) => o.unknown.push(item.to_string()),
                },
                ("+", "exitstorm", Some(v)) => match v.parse::<u32>() {
                    Ok(x) if x >= 1 => o.exitstorm = Some(x),
                    _ => o.unknown.push(item.to_string()),
                },
                _ => o.unknown.push(item.to_string()),
            }
        }
        o
    }
}

/// Tier 1.5 configuration of a state.
#[derive(Clone, Debug)]
pub struct T15Config {
    /// Mode.
    pub mode: T15Mode,
    /// Promotion threshold in back-edges per nest.
    pub hot: i32,
    /// Pass switches.
    pub opt: OptFlags,
}

impl T15Config {
    /// The configuration for `mode` with the default threshold (`Always`: 1).
    pub fn for_mode(mode: T15Mode) -> T15Config {
        T15Config {
            mode,
            hot: if mode == T15Mode::Always {
                1
            } else {
                T15_HOT_DEFAULT
            },
            opt: OptFlags::default(),
        }
    }

    /// The pure part of the env parsing: values of `RUSTYNAIL_JIT_T15`, `RUSTYNAIL_JIT_T15_HOT`,
    /// `RUSTYNAIL_JIT_OPT`.
    pub fn parse(mode: Option<&str>, hot: Option<&str>, opt: Option<&str>) -> T15Config {
        let mode = match mode.map(|m| m.trim().to_ascii_lowercase()).as_deref() {
            Some("on" | "1" | "true") => T15Mode::On,
            Some("always") => T15Mode::Always,
            _ => T15Mode::Off,
        };
        let mut c = T15Config::for_mode(mode);
        if let Some(h) = hot.and_then(|h| h.trim().parse::<i32>().ok()) {
            if mode != T15Mode::Always {
                c.hot = h.max(1);
            }
        }
        if let Some(o) = opt {
            c.opt = OptFlags::parse(o);
        }
        c
    }

    /// The process-wide configuration from the environment (read once).
    pub fn process() -> T15Config {
        static C: std::sync::OnceLock<T15Config> = std::sync::OnceLock::new();
        C.get_or_init(|| {
            T15Config::parse(
                std::env::var("RUSTYNAIL_JIT_T15").ok().as_deref(),
                std::env::var("RUSTYNAIL_JIT_T15_HOT").ok().as_deref(),
                std::env::var("RUSTYNAIL_JIT_OPT").ok().as_deref(),
            )
        })
        .clone()
    }
}

// ---- nests -----------------------------------------------------------------------------------------------

/// Candidate loop nests of `code`: the outermost loops, overlapping loops merged, as `(head, end)` with `end` the
/// last back-edge instruction. This is the T3 placeholder of the nest search; T7 replaces it with the analysis of
/// `t15::analysis` (single entry, size cap), the counters and the warm-up code do not care how nests are found.
/// Back-edges are found by the rules of `lower`: `FORLOOP`/`TFORLOOP` (target `pc + 1 - Bx`), a backward `JMP`,
/// and a conditional followed by a backward `JMP`.
pub fn scan_nests(code: &[u32]) -> Vec<(u32, u32)> {
    let n = code.len();
    let mut loops: Vec<(u32, u32)> = Vec::new();
    for pc in 0..n {
        let i = Instruction(code[pc]);
        let p = pc as i64;
        let t = match i.opcode() {
            Some(OpCode::Forloop | OpCode::Tforloop) => Some(p + 1 - i64::from(i.bx())),
            Some(OpCode::Jmp) if i.sj() < 0 => Some(p + 1 + i64::from(i.sj())),
            Some(
                OpCode::Eq
                | OpCode::Lt
                | OpCode::Le
                | OpCode::Eqk
                | OpCode::Eqi
                | OpCode::Lti
                | OpCode::Lei
                | OpCode::Gti
                | OpCode::Gei
                | OpCode::Test
                | OpCode::Testset,
            ) if pc + 1 < n => {
                let j = Instruction(code[pc + 1]);
                (j.opcode() == Some(OpCode::Jmp) && j.sj() < 0).then(|| p + 2 + i64::from(j.sj()))
            }
            _ => None,
        };
        if let Some(t) = t {
            if (0..=p).contains(&t) {
                loops.push((t as u32, pc as u32));
            }
        }
    }
    loops.sort_unstable_by_key(|&(h, e)| (h, std::cmp::Reverse(e)));
    let mut nests: Vec<(u32, u32)> = Vec::new();
    for (h, e) in loops {
        match nests.last_mut() {
            Some(last) if h <= last.1 => last.1 = last.1.max(e),
            _ => nests.push((h, e)),
        }
    }
    nests
}

/// Bytes of Rust heap [`T15Data`] and the per-prototype tier 1.5 tables hold for the life of the descriptor
/// (charged with it, refunded with it): profile bytes, nest states, the block signature, the pc map, version 0.
pub fn data_bytes(n: usize, nests: usize) -> usize {
    std::mem::size_of::<T15Data>()
        + std::mem::size_of::<T15Proto>()
        + n // prof
        + n * 8 // cprof
        + nests * std::mem::size_of::<NestState>()
        + n * (1 + 4 + 4) // block signature: head, cnt_at, rem
        + n * 4 // resume pcs bound
        + n * 4 // pc map
        + std::mem::size_of::<VersionData>()
}

// ---- warm-up description handed to `lower` ----------------------------------------------------------------

/// One still-warm nest as `lower` sees it.
#[derive(Clone, Copy, Debug)]
pub struct WarmNest {
    /// Index in [`T15Data::nests`].
    pub id: u32,
    /// First instruction of the nest.
    pub head: u32,
    /// Last back-edge instruction of the nest.
    pub end: u32,
    /// Address of the nest's promote counter.
    pub ctr_addr: usize,
}

/// What `lower` needs to instrument tier-1 code (see `LowerCfg::t15_warmup`).
#[derive(Clone, Debug, Default)]
pub struct Warmup {
    /// Version id of the code being generated (passed back to the helpers).
    pub ver: u8,
    /// Address of [`jit_t15_promote`].
    pub promote_fn: u64,
    /// Address of `prof[0]`.
    pub prof_addr: usize,
    /// Address of `cprof[0]` (the payload of the light C function seen in the callee slot of each `CALL`).
    pub cprof_addr: usize,
    /// Address of [`T15Data::cont`].
    pub cont_addr: usize,
    /// The nests still warm in this version.
    pub nests: Vec<WarmNest>,
    /// J5/F5 (TR19b(b)): the trace dispatch owns the back-edges of the heads. The promote counter is not put on the
    /// edge; its stub `W_H` is reached through the head's dispatch cell (`RegionEnv::warm_stub`).
    pub via_dispatch: bool,
}

impl Warmup {
    /// Does a warm nest contain `pc`?
    pub fn in_warm_nest(&self, pc: u32) -> bool {
        self.nests.iter().any(|n| (n.head..=n.end).contains(&pc))
    }
    /// The warm nest (index into `nests`) that contains the back-edge instruction `pc` going to `head`.
    pub fn nest_of(&self, pc: u32, head: u32) -> Option<usize> {
        self.nests
            .iter()
            .position(|n| n.head <= head && pc <= n.end)
    }
}

// ---- data ------------------------------------------------------------------------------------------------

/// Lifecycle of a nest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NestPhase {
    /// Counting back-edges in tier-1 warm-up code.
    Warm,
    /// Promoted: the current code version was built for it (no warm-up code for this nest any more).
    Promoted,
    /// Given up (permanent failure, compile cap, back-off exhausted). Never re-tried (docs/JIT_TIER15.md 3.8).
    Off,
}

/// State of one nest. Lives in a `Box<[NestState]>` of fixed length: the addresses of the counters are embedded in
/// machine code (B4: no growable `Vec`).
#[derive(Debug)]
pub struct NestState {
    /// First instruction.
    pub head: u32,
    /// Last back-edge instruction.
    pub end: u32,
    /// Promote counter: back-edges left (decremented by machine code; negative = fire).
    pub ctr: Cell<i32>,
    /// Phase.
    pub phase: Cell<NestPhase>,
    /// Respecialisations (T7): 0 = the first speculation, `GEN_CAP` = no more.
    pub gen: Cell<u8>,
    /// Back-off exponent of refused promotions.
    pub backoff: Cell<u8>,
    /// Stub failures (non-peel) accounted so far, summed over policy decisions (T7).
    pub entry_fail: Cell<u32>,
    /// Successful entries through `stub(H)` since the last policy decision (T7; incremented by machine code).
    pub entry_ok: Cell<u32>,
    /// Tags of the frame slots `[0, maxstacksize)` when the nest was promoted (the entry speculation of its
    /// region; kept for the re-lowerings that follow other promotions and respecialisations).
    pub obs: RefCell<Box<[u8]>>,
    /// Stub-failure countdown (T7): decremented by the failure pads of the nest's stubs; the helper is called when it
    /// goes negative. Shared by every code version of the prototype (stale versions write here too, which is
    /// harmless: B4).
    pub fail: Cell<i32>,
    /// The value `fail` was last armed with (the number of failures one decision stands for).
    pub armed: Cell<i32>,
    /// Policy decisions taken so far (penalty exponent of benign failure rates).
    pub fires: Cell<u8>,
    /// Exit feedback and the stub table of the current version (allocated on first use: a nest costs 16 bytes of
    /// descriptor memory until it has a region).
    pub fb: RefCell<Option<Box<NestFb>>>,
}

/// Exit feedback of a nest (see [`NestState::fb`]).
#[derive(Debug, Default)]
pub struct NestFb {
    /// Entry-fact widenings from exit feedback `(slot, set)` (docs 3.8; `Speculation::entry_widen`).
    pub entry_widen: Vec<(u8, super::TagSet)>,
    /// Def-site widenings `(pc, set)` (`Speculation::def_widen`).
    pub def_widen: Vec<(u32, super::TagSet)>,
    /// The stubs of the nest's region in the *current* code version (what the failure pads guard).
    pub sites: Vec<StubSite>,
}

impl NestState {
    /// The feedback record, created on first use.
    pub fn fb_mut(&self) -> std::cell::RefMut<'_, NestFb> {
        std::cell::RefMut::map(self.fb.borrow_mut(), |o| {
            &mut **o.get_or_insert_with(Box::default)
        })
    }
}

/// One guard of a stub: slot, expected tag byte, and whether a failure is the by-design peel failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StubGuard {
    /// Lua register.
    pub slot: u8,
    /// Expected tag byte (variant bits included).
    pub tag: u8,
    /// A failure of this guard is the expected first-entry failure of a peeled slot (not counted).
    pub peel: bool,
    /// The slot's fact at this stub comes from the load at this pc (its `guard_def`, i.e. `prof[pc]`), if any.
    pub def_pc: Option<u32>,
    /// The fact comes from a guard that exit feedback cannot change (`LEN`, `POW`): a failure is a no-progress case.
    pub fixed: bool,
}

/// A stub with failure accounting (the guards it checks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StubSite {
    /// The stub's pc.
    pub pc: u32,
    /// Its guards.
    pub guards: Vec<StubGuard>,
}

/// Addresses and ids that a failure pad needs (embedded in machine code; all address-stable, B4).
#[derive(Clone, Copy, Debug)]
pub struct FeedbackSpec {
    /// Code version being generated.
    pub ver: u8,
    /// Nest id.
    pub nest: u32,
    /// Address of [`NestState::fail`].
    pub fail_addr: usize,
    /// Address of [`NestState::entry_ok`].
    pub ok_addr: usize,
    /// Address of [`T15Data::cont`].
    pub cont_addr: usize,
    /// Address of [`jit_t15_event`].
    pub event_fn: u64,
}

/// Per-nest inputs and outputs of the policy for one lowering (see [`lower_with_regions_ex`]).
#[derive(Clone, Debug, Default)]
pub struct NestPolicy {
    /// Entry widenings.
    pub entry_widen: Vec<(u8, super::TagSet)>,
    /// Def widenings.
    pub def_widen: Vec<(u32, super::TagSet)>,
    /// Failure-pad addresses (`None`: no feedback code, `-respec`).
    pub fb: Option<FeedbackSpec>,
}

/// What the policy hands to a lowering (`inputs`) and gets back (`sites`).
#[derive(Clone, Debug, Default)]
pub struct PolicyCtl {
    /// Indexed by nest id.
    pub nests: Vec<NestPolicy>,
    /// Output: `(nest id, site)` of every stub emitted with failure accounting.
    pub sites: Vec<(u32, StubSite)>,
}

/// Block signature plus the address-stable data of one prototype. Never moved, never grown, freed with the
/// descriptor (and with it every code version that embeds addresses into it).
#[derive(Debug)]
pub struct T15Data {
    /// Last observed result tag per pc (`0xFF` = never).
    pub prof: Box<[Cell<u8>]>,
    /// Last light C function called at each `CALL` pc (0 = none / not a light C function): intrinsic sites.
    pub cprof: Box<[Cell<usize>]>,
    /// `CALL` pcs whose intrinsic form was demoted by the policy (the site compiles as an ordinary `CALL` from now on).
    pub intr_demoted: RefCell<Vec<u32>>,
    /// The nests found when the prototype was first compiled.
    pub nests: Box<[NestState]>,
    /// Continuation address written by the helpers (see the module docs).
    pub cont: Cell<u64>,
    /// Compiles so far (tier 1 counts).
    pub compiles: Cell<u8>,
    /// Block structure of version 0; every later version must equal it.
    pub sig: OnceCell<crate::lower::BlockSig>,
    /// Promotion threshold used for counter (re)arming.
    pub hot: i32,
    rng: Cell<u32>,
}

impl T15Data {
    /// The `CALL` sites whose callee slot held a registered `math` builtin when last seen by the warm-up code.
    pub fn intr_sites(&self) -> Vec<super::intrinsics::Site> {
        self.cprof
            .iter()
            .enumerate()
            .filter_map(|(pc, c)| {
                if self.intr_demoted.borrow().contains(&(pc as u32)) {
                    return None;
                }
                let k = rn_core::jit_hooks::intrinsic_of(c.get())?;
                Some((pc as u32, super::intrinsics::Intr::from_index(k)?))
            })
            .collect()
    }

    /// Data for a prototype of `n` instructions with the given nests.
    pub fn new(n: usize, nests: &[(u32, u32)], cfg: &T15Config) -> Box<T15Data> {
        Box::new(T15Data {
            prof: (0..n).map(|_| Cell::new(0xFF)).collect(),
            cprof: (0..n).map(|_| Cell::new(0)).collect(),
            intr_demoted: RefCell::new(Vec::new()),
            nests: nests
                .iter()
                .map(|&(head, end)| NestState {
                    head,
                    end,
                    ctr: Cell::new(cfg.hot - 1),
                    phase: Cell::new(NestPhase::Warm),
                    gen: Cell::new(0),
                    backoff: Cell::new(0),
                    entry_fail: Cell::new(0),
                    entry_ok: Cell::new(0),
                    obs: RefCell::new(Box::new([])),
                    fail: Cell::new(i32::MAX),
                    armed: Cell::new(0),
                    fires: Cell::new(0),
                    fb: RefCell::new(None),
                })
                .collect(),
            cont: Cell::new(0),
            compiles: Cell::new(1),
            sig: OnceCell::new(),
            hot: cfg.hot,
            rng: Cell::new(0x9E37_79B9),
        })
    }

    /// The warm-up description for code version `ver`: the nests that are still `Warm` and not being promoted by
    /// `plans`.
    pub fn warmup(&self, ver: u8, plans: &[NestPlan]) -> Warmup {
        Warmup {
            via_dispatch: false,
            ver,
            promote_fn: jit_t15_promote as *const () as usize as u64,
            prof_addr: self.prof.as_ptr() as usize,
            cprof_addr: self.cprof.as_ptr() as usize,
            cont_addr: self.cont.as_ptr() as usize,
            nests: self
                .nests
                .iter()
                .enumerate()
                .filter(|(id, n)| {
                    n.phase.get() == NestPhase::Warm && !plans.iter().any(|p| p.nest == *id as u32)
                })
                .map(|(id, n)| WarmNest {
                    id: id as u32,
                    head: n.head,
                    end: n.end,
                    ctr_addr: n.ctr.as_ptr() as usize,
                })
                .collect(),
        }
    }

    fn rand4(&self) -> i32 {
        let mut x = self.rng.get();
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng.set(x);
        (x & 15) as i32
    }
}

/// Per-version data, owned by the version's code block (B4): exit/miss counters of the sites of *that* code.
/// Stub code of a retired version keeps writing here, never into the data of a newer version. Freed (with the
/// retired block) only when the prototype is freed.
#[derive(Debug)]
pub struct VersionData {
    /// Version id (index in [`T15Proto::versions`]).
    pub id: u8,
    /// Type-guard exit counters per site.
    pub exit_ctr: Box<[Cell<u32>]>,
    /// Miss exit counters per site.
    pub miss_ctr: Box<[Cell<u32>]>,
}

impl VersionData {
    /// Data for version `id` with `sites` exit sites.
    pub fn new(id: u8, sites: usize) -> VersionData {
        VersionData {
            id,
            exit_ctr: (0..sites).map(|_| Cell::new(0)).collect(),
            miss_ctr: (0..sites).map(|_| Cell::new(0)).collect(),
        }
    }
}

/// The tier 1.5 part of a descriptor.
#[derive(Debug)]
pub struct T15Proto {
    /// Address-stable data.
    pub data: Box<T15Data>,
    /// Data of every version, indexed by version id (boxes: stable addresses).
    pub versions: Vec<Box<VersionData>>,
    /// Blocks of the older versions (index = version id), kept mapped until the prototype is freed.
    pub retired: Vec<crate::mem::CodeBlock>,
    /// Native offset + 1 of the tier-1 label of every pc in the *current* block (0 = dead pc, M9).
    pub pcmap: Vec<u32>,
    /// Current version id.
    pub cur: u8,
}

impl T15Proto {
    /// Tier 1.5 state of a freshly compiled prototype (version 0).
    pub fn new(data: Box<T15Data>, low: &Lowered, em: &Emitted) -> T15Proto {
        let _ = data.sig.set(low.sig.clone());
        T15Proto {
            data,
            versions: vec![Box::new(VersionData::new(0, 0))],
            retired: Vec::new(),
            pcmap: pc_map(low, em),
            cur: 0,
        }
    }
}

/// Native offset + 1 per pc of the tier-1 labels (0 for dead pcs).
pub fn pc_map(low: &Lowered, em: &Emitted) -> Vec<u32> {
    low.pc_labels
        .iter()
        .map(|l| {
            l.and_then(|l| em.label_off.get(l as usize).copied().flatten())
                .map_or(0, |off| off as u32 + 1)
        })
        .collect()
}

/// P-X: like [`pc_map`] for the labels trace exits land on (`Lowered::land_labels`).
pub fn land_map(low: &Lowered, em: &Emitted) -> Vec<u32> {
    low.land_labels
        .iter()
        .map(|l| {
            l.and_then(|l| em.label_off.get(l as usize).copied().flatten())
                .map_or(0, |off| off as u32 + 1)
        })
        .collect()
}

// ---- lowering --------------------------------------------------------------------------------------------

/// A promoted nest (T3 placeholder of the region plan of `t15::analysis`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NestPlan {
    /// Index in [`T15Data::nests`].
    pub nest: u32,
    /// First instruction.
    pub head: u32,
    /// Last back-edge instruction.
    pub end: u32,
}

/// What the region generator needs besides the code and the constants (docs/JIT_TIER15.md section 16).
#[derive(Clone, Debug)]
pub struct FuncMeta {
    /// `maxstacksize`.
    pub max_stack: u32,
    /// Slots captured by a closure of the function (`instack` upvalue descriptions of the nested prototypes).
    pub escaped: super::SlotSet,
    /// Frame tags at promotion, indexed by nest id (empty = not promoted before).
    pub obs: Vec<Vec<u8>>,
    /// The `prof[pc]` bytes.
    pub prof: Vec<u8>,
    /// Intrinsic call sites ([`super::Policy::intr`]).
    pub intr: Vec<super::intrinsics::Site>,
    /// `RUSTYNAIL_JIT_OPT`.
    pub opt: OptFlags,
}

/// Tier-1 lowering plus the typed regions of `plans` (docs/JIT_TIER15.md section 16). A nest whose analysis or
/// code generation refuses gets no region (the program is then tier 1 again for it). The block structure is
/// identical in every case (`rt::recompile` asserts it, section 12 condition 3).
pub fn lower_with_regions(
    code: &[u32],
    cfg: LowerCfg<'_>,
    plans: &[NestPlan],
    meta: Option<&FuncMeta>,
) -> Option<Lowered> {
    lower_with_regions_ex(code, cfg, plans, meta, None)
}

/// [`lower_with_regions`] with the policy inputs (widenings, failure-pad addresses) and the stub-site output of
/// `ctl` (docs/JIT_TIER15.md 21).
pub fn lower_with_regions_ex(
    code: &[u32],
    cfg: LowerCfg<'_>,
    plans: &[NestPlan],
    meta: Option<&FuncMeta>,
    ctl: Option<&mut PolicyCtl>,
) -> Option<Lowered> {
    lower_with_regions_hook(code, cfg, plans, meta, ctl, None)
}

/// [`lower_with_regions_ex`] plus a second hook that runs after the region hook (J5/F5: the trace dispatch of the
/// nests that are not regions, TR19b(b)). The hooks must not claim the same heads.
pub(crate) fn lower_with_regions_hook(
    code: &[u32],
    cfg: LowerCfg<'_>,
    plans: &[NestPlan],
    meta: Option<&FuncMeta>,
    mut ctl: Option<&mut PolicyCtl>,
    mut extra: Option<&mut crate::lower::RegionHook<'_>>,
) -> Option<Lowered> {
    let Some(meta) = meta.filter(|_| !plans.is_empty()) else {
        return lower_ex(code, cfg, extra, false);
    };
    let k: Vec<super::TagSet> = cfg
        .consts
        .iter()
        .map(|v| super::TagSet::of_tag(v.tag()))
        .collect();
    let k_int: Vec<Option<i64>> = cfg
        .consts
        .iter()
        .map(|v| {
            (v.tag() == rn_core::value::LUA_VNUMINT)
                .then(|| v.as_int())
                .flatten()
        })
        .collect();
    let mut hook = |cx: &mut Cx<'_>, env: &RegionEnv<'_>| -> RegionOut {
        let mut out = RegionOut::default();
        'regions: {
            let info = super::FuncInfo {
                code,
                k: k.clone(),
                k_int: k_int.clone(),
                max_stack: meta.max_stack,
                escaped: meta.escaped,
                extra_heads: env.head.to_vec(),
            };
            let Ok(an) = super::analysis::Analysis::new(&info) else {
                break 'regions;
            };
            let policy = super::Policy {
                intr: meta.intr.clone(),
                ..super::Policy::default()
            };
            for p in plans {
                let sel = an.select_nest(p.head, p.end, &policy);
                let obs: &[u8] = meta.obs.get(p.nest as usize).map_or(&[], Vec::as_slice);
                let np: Option<NestPolicy> = ctl
                    .as_ref()
                    .and_then(|c| c.nests.get(p.nest as usize))
                    .cloned();
                let (ew, dw, fb) = np.map_or((Vec::new(), Vec::new(), None), |n| {
                    (n.entry_widen, n.def_widen, n.fb)
                });
                let spec = super::Speculation {
                    obs,
                    prof: &meta.prof,
                    entry_widen: &ew,
                    def_widen: &dw,
                };
                let plan = match sel.and_then(|l| an.plan(l, &spec, &policy)) {
                    Ok(pl) => pl,
                    Err(e) => {
                        if rt::verbose() {
                            eprintln!("[t15] nest [{}, {}]: no region ({e:?})", p.head, p.end);
                        }
                        continue;
                    }
                };
                if let Some(r) = super::codegen::emit_region(cx, env, &plan, &meta.opt, fb) {
                    if let Some(c) = ctl.as_deref_mut() {
                        c.sites.extend(r.sites.into_iter().map(|s| (p.nest, s)));
                    }
                    out.points.extend(r.points);
                    if meta.opt.shadow {
                        for &(hpc, l) in &r.heads {
                            super::verify::add_stub_head(l, hpc);
                        }
                    }
                    out.heads.extend(r.heads);
                    out.overrides.extend(r.overrides);
                }
            }
        }
        if let Some(x) = extra.as_mut() {
            let o = x(cx, env);
            out.points.extend(o.points);
            out.heads.extend(o.heads);
            out.overrides.extend(o.overrides);
            out.sites.extend(o.sites);
            out.resume_overrides.extend(o.resume_overrides);
        }
        out
    };
    let low = lower_ex(code, cfg, Some(&mut hook), meta.opt.verify);
    if meta.opt.shadow {
        super::verify::finish_lowering(low.as_ref().map(|l| &l.prog));
    }
    low
}

// ---- helpers called from compiled code -------------------------------------------------------------------

/// What an event is about (`jit_t15_event`, argument `b`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    /// A type-guard exit counter of a site reached its limit.
    Exit = 0,
    /// A miss exit counter reached its limit.
    Miss = 1,
    /// Respecialise: recompile the prototype with the current plans (T7 changes the plans first).
    Respecialize = 2,
    /// The failure countdown of a nest fired: its stub at the target pc failed [`NestState::armed`] times (T7).
    StubFail = 3,
}

fn descriptor(st: &State, ci: Ci) -> Option<(Gc<Proto>, *mut JitProto)> {
    let cl = rt::proto_of(st, ci)?;
    let p = cl.proto();
    let jp = p.jit.get() as usize;
    (jp > NOJIT).then_some((p, jp as *mut JitProto))
}

/// Absolute continuation address of `pc` in the current code (M9: `None` for dead pcs and out-of-range pcs).
fn continuation(jp: &JitProto, t: &T15Proto, pc: u32) -> Option<u64> {
    let off = *t.pcmap.get(pc as usize)?;
    (off != 0).then(|| jp.block.addr() as u64 + u64::from(off - 1))
}

/// `jit_t15_promote(thread, ci, nest | ver << 16, head) -> 0 | 1`: called from the warm-up code of version `ver`
/// at a back-edge of nest `nest` to loop head `head` when the nest's counter fired, *before* the head's
/// `CountSub`, with the frame exact (tier 1 keeps everything in the slots). 1: continue at the address in
/// `T15Data::cont` (the tier-1 label of `head` in the current code); 0: continue in the code you are in.
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_t15_promote(
    th: *mut Thread,
    ci: *mut CallInfo,
    a: u32,
    head: u32,
) -> u32 {
    // SAFETY: `th` is the live thread pinned in `r12`, `ci` its current frame (`r13`).
    let (mut st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    u32::from(promote(&mut st, ci, a >> 16, a & 0xFFFF, head))
}

/// `jit_t15_event(thread, ci, site | ver << 24, kind << 16 | target_pc) -> 0 | 1` (exit stubs of regions, T4+).
/// Counters are those of the version `ver` that the calling code belongs to. 1: continue at `T15Data::cont`, the
/// tier-1 label of `target_pc` in the current code; 0: use the label of the code you are in.
///
/// # Safety
/// Called only from compiled code with the running thread and its current Lua frame.
pub unsafe extern "C" fn jit_t15_event(th: *mut Thread, ci: *mut CallInfo, a: u32, b: u32) -> u32 {
    // SAFETY: as in `jit_t15_promote`.
    let (mut st, ci) = unsafe { (jit_hooks::state_from_thread(th.cast()), Ci::from_raw(ci)) };
    let kind = match (b >> 16) & 0xFF {
        0 => EventKind::Exit,
        1 => EventKind::Miss,
        3 => EventKind::StubFail,
        _ => EventKind::Respecialize,
    };
    u32::from(event(
        &mut st,
        ci,
        a >> 24,
        a & 0x00FF_FFFF,
        kind,
        b & 0xFFFF,
    ))
}

/// J5/F5 (TR19): tells the trace table of `jp` that nest `nest_id` is no longer the tracer's J4 stub: promoted (every
/// head `Black`, the region owns the loop) or given up. No-op without traces.
#[cfg(feature = "trace")]
fn tr_close(st: &mut State, jp: &JitProto, nest_id: u32, promoted: bool) {
    if let Some(tp) = jp.tr.as_deref() {
        // SAFETY: the per-state data is live.
        let d = unsafe { &*rt::data_ptr(st) };
        tp.close_nest(
            nest_id as usize,
            promoted,
            d.tr.cfg.pref == crate::tr::cache::TrPref::Region,
        );
    }
}
#[cfg(not(feature = "trace"))]
fn tr_close(_: &mut State, _: &JitProto, _: u32, _: bool) {}

/// Is nest `nest_id` of `jp` the tracer's to give away (TR19)? Always true without a trace table.
#[cfg(feature = "trace")]
fn tr_eligible(jp: &JitProto, nest_id: u32) -> bool {
    jp.tr
        .as_deref()
        .is_none_or(|tp| tp.nest_eligible(nest_id as usize))
}
#[cfg(not(feature = "trace"))]
fn tr_eligible(_: &JitProto, _: u32) -> bool {
    true
}

fn refuse_backoff(t: &T15Data, nest: &NestState, st: &mut State) {
    let k = nest.backoff.get() + 1;
    nest.backoff.set(k);
    // SAFETY: the per-state data is live (see `rt::set_mode`).
    let d = unsafe { &mut *rt::data_ptr(st) };
    d.stats.t15_refused += 1;
    if k > MAX_BACKOFF {
        turn_off(d, nest);
    } else {
        let hot = i64::from(t.hot) << k;
        nest.ctr
            .set((hot.min(i64::from(i32::MAX / 2)) as i32) + t.rand4());
    }
}

fn turn_off(d: &mut rt::JitData, nest: &NestState) {
    if rt::verbose() {
        eprintln!("[t15] nest [{}, {}] is off", nest.head, nest.end);
    }
    nest.phase.set(NestPhase::Off);
    // A8/A5: the warm-up counter of the running version stays in its code; it can never fire again (and the
    // next recompile of this prototype, for another nest, no longer contains it).
    nest.ctr.set(i32::MAX);
    d.stats.t15_off += 1;
}

fn promote(st: &mut State, ci: Ci, ver: u32, nest_id: u32, head: u32) -> bool {
    let Some((p, jpp)) = descriptor(st, ci) else {
        return false;
    };
    // SAFETY: the descriptor of the running frame's prototype is live while the frame exists; no other reference
    // into it is held (the helper runs no generated code until it returns).
    let jp = unsafe { &*jpp };
    let Some(t) = jp.t15.as_deref() else {
        return false;
    };
    // `T15Data` is address-stable and lives as long as the descriptor, which outlives this call; the reference
    // is taken once so that no `&T15Proto` is held across the recompile (which mutates the `T15Proto`).
    let cur = u32::from(t.cur);
    // SAFETY: see above.
    let data: &T15Data = unsafe { &*std::ptr::from_ref::<T15Data>(&t.data) };
    let Some(nest) = data.nests.get(nest_id as usize) else {
        return false;
    };
    if ver > cur {
        return false; // not a version of this prototype: ignore
    }
    match nest.phase.get() {
        NestPhase::Promoted => {
            // idempotent: an activation still running in an older version reaches the back-edge of a nest that a
            // nested activation already promoted. Move it to the current code at the same loop head.
            nest.ctr.set(i32::MAX);
            if ver == cur {
                return false;
            }
            return match continuation(jp, t, head) {
                Some(addr) => {
                    data.cont.set(addr);
                    true
                }
                None => false,
            };
        }
        NestPhase::Off => {
            nest.ctr.set(i32::MAX);
            return false;
        }
        NestPhase::Warm => {}
    }
    if !tr_eligible(jp, nest_id) {
        // (TR19: a head of the nest is being recorded or has a trace: the tracer owns the nest for now)
        nest.ctr.set(data.hot.saturating_add(data.rand4()));
        return false;
    }
    let mut plans: Vec<NestPlan> = data
        .nests
        .iter()
        .enumerate()
        .filter(|(_, n)| n.phase.get() == NestPhase::Promoted)
        .map(|(id, n)| NestPlan {
            nest: id as u32,
            head: n.head,
            end: n.end,
        })
        .collect();
    plans.push(NestPlan {
        nest: nest_id,
        head: nest.head,
        end: nest.end,
    });
    // entry speculation of the region: the tags of the frame now (tier 1 keeps every value in its slot)
    {
        let func = ci.r().func.0;
        let obs: Box<[u8]> = (0..usize::from(p.maxstacksize))
            .map(|s| st.get(rn_core::state::StkId(func + 1 + s)).tag())
            .collect();
        *nest.obs.borrow_mut() = obs;
    }
    match rt::recompile(st, &p, jpp, &plans) {
        Ok(()) => {
            nest.phase.set(NestPhase::Promoted);
            nest.ctr.set(i32::MAX);
            // SAFETY: re-borrow of the live descriptor (the recompile swapped its tables, not the box).
            tr_close(st, unsafe { &*jpp }, nest_id, true);
            arm_failure_counter(st, nest);
            // SAFETY: the per-state data is live.
            let d = unsafe { &mut *rt::data_ptr(st) };
            d.stats.t15_promoted += 1;
            // SAFETY: re-borrow after the recompile (the descriptor is the same box; its tables were swapped).
            let jp = unsafe { &*jpp };
            let t = jp.t15.as_deref().expect("still there");
            if rt::verbose() {
                eprintln!(
                    "[t15] promoted nest {nest_id} [{}, {}] of proto@{:x}: version {ver} -> {}, {} retired blocks",
                    nest.head,
                    nest.end,
                    p.code_ptr() as usize,
                    t.cur,
                    t.retired.len()
                );
            }
            match continuation(jp, t, head) {
                Some(addr) => {
                    t.data.cont.set(addr);
                    true
                }
                None => false,
            }
        }
        Err(Fail::Retry) => {
            refuse_backoff(data, nest, st);
            if nest.phase.get() == NestPhase::Off {
                // SAFETY: as above.
                tr_close(st, unsafe { &*jpp }, nest_id, false);
            }
            false
        }
        Err(Fail::Permanent) => {
            // SAFETY: the per-state data is live.
            let d = unsafe { &mut *rt::data_ptr(st) };
            turn_off(d, nest);
            // SAFETY: as above.
            tr_close(st, unsafe { &*jpp }, nest_id, false);
            false
        }
    }
}

fn event(st: &mut State, ci: Ci, ver: u32, site: u32, kind: EventKind, target: u32) -> bool {
    let Some((p, jpp)) = descriptor(st, ci) else {
        return false;
    };
    // SAFETY: as in `promote`.
    let jp = unsafe { &*jpp };
    let Some(t) = jp.t15.as_deref() else {
        return false;
    };
    // site data of the version the caller belongs to (and only that one): a stale id never touches newer data
    let Some(vd) = t.versions.get(ver as usize) else {
        return false;
    };
    let stale = ver != u32::from(t.cur);
    match kind {
        EventKind::Exit => {
            if let Some(c) = vd.exit_ctr.get(site as usize) {
                c.set(c.get().wrapping_add(1));
            }
        }
        EventKind::Miss => {
            if let Some(c) = vd.miss_ctr.get(site as usize) {
                c.set(c.get().wrapping_add(1));
            }
        }
        EventKind::StubFail => {
            // (a stale version's pad only moves its activation to the current code: the decision is the current
            // version's business)
            if !stale {
                stub_fail(st, ci, &p, jpp, site, target);
            } else {
                // SAFETY: the per-state data is live.
                unsafe { &mut *rt::data_ptr(st) }.stats.t15_stale_events += 1;
            }
            if stale && !super::mutants::active("pol_stale_no_rearm") {
                // the countdown fired in a stale version's pad: it is below zero now; re-arm it, or the current
                // version's failures would never reach the policy again
                if let Some(n) = t.data.nests.get(site as usize) {
                    if n.phase.get() == NestPhase::Promoted {
                        n.fail.set(n.armed.get().max(1) - 1);
                    }
                }
            }
        }
        EventKind::Respecialize => {
            if !stale {
                let plans: Vec<NestPlan> = t
                    .data
                    .nests
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.phase.get() == NestPhase::Promoted)
                    .map(|(id, n)| NestPlan {
                        nest: id as u32,
                        head: n.head,
                        end: n.end,
                    })
                    .collect();
                // failure leaves the current version running; the stub then jumps to its own label
                let _ = rt::recompile(st, &p, jpp, &plans);
            }
        }
    }
    // SAFETY: re-borrow (a recompile replaces `block` and the tables, not the box).
    let jp = unsafe { &*jpp };
    let t = jp.t15.as_deref().expect("still there");
    if ver == u32::from(t.cur) {
        return false; // same code: the caller's own tier-1 label is right
    }
    match continuation(jp, t, target) {
        Some(addr) => {
            t.data.cont.set(addr);
            true
        }
        None => false, // dead pc (M9): refuse, the caller uses its own label
    }
}

// ---- policy: stub failures, respecialisation, give-up (T7; docs/JIT_TIER15.md 21) ---------------------------

/// Arms the failure countdown of a freshly promoted nest.
fn arm_failure_counter(st: &mut State, nest: &NestState) {
    // SAFETY: the per-state data is live (see `rt::set_mode`).
    let hot = unsafe { &*rt::data_ptr(st) }
        .t15
        .opt
        .exit_hot
        .unwrap_or(EXIT_HOT_DEFAULT);
    nest.armed.set(hot);
    nest.fail.set(hot - 1);
    nest.entry_ok.set(0);
}

/// The failure countdown of nest `nest_id` fired in the stub at `pc` of the current code version. Decides, in this
/// order: benign failure rate at the head stub (penalty: the next decision needs twice as many failures); gen cap
/// reached (give the region up); otherwise respecialise (gen 0: *retarget* the speculation to what the frame holds
/// now; later: *widen* it with the observed tags) and recompile. A failed recompile leaves the running version
/// (the cap on total compiles) and parks the countdown.
fn stub_fail(st: &mut State, ci: Ci, p: &Gc<Proto>, jpp: *mut JitProto, nest_id: u32, pc: u32) {
    // SAFETY: as in `promote`; no `&T15Proto` is held across the recompile below.
    let jp = unsafe { &*jpp };
    let Some(t) = jp.t15.as_deref() else {
        return;
    };
    // SAFETY: `T15Data` is address-stable and outlives this call.
    let data: &T15Data = unsafe { &*std::ptr::from_ref::<T15Data>(&t.data) };
    let Some(nest) = data.nests.get(nest_id as usize) else {
        return;
    };
    // SAFETY: the per-state data is live.
    let d = unsafe { &mut *rt::data_ptr(st) };
    d.stats.t15_stub_events += 1;
    if nest.phase.get() != NestPhase::Promoted {
        nest.fail.set(i32::MAX);
        return;
    }
    let hot = d.t15.opt.exit_hot.unwrap_or(EXIT_HOT_DEFAULT);
    let armed = nest.armed.get().max(1);
    // (cumulative since the last respecialisation: one bad call of many iterations fires several times)
    let oks = nest.entry_ok.get();
    let fails = nest.entry_fail.get().saturating_add(armed as u32);
    nest.entry_fail.set(fails);
    nest.fires.set(nest.fires.get().saturating_add(1));
    let rearm = |n: i32| {
        nest.armed.set(n);
        nest.fail.set(n - 1);
    };
    if pc == nest.head
        && i64::from(fails) * 4 <= i64::from(oks)
        && !super::mutants::active("pol_no_penalty")
    {
        // thrash detection (enter/exit ratio): the head stub fails for a small fraction of its entries; the region
        // is doing its job. Penalty: ask for twice as many failures before the next decision.
        d.stats.t15_penalties += 1;
        rearm(armed.saturating_mul(2).min(1 << 24));
        if rt::verbose() {
            eprintln!(
                "[t15] nest [{}, {}]: {fails} failures at pc {pc} against {oks} entries: benign, next decision after {}",
                nest.head,
                nest.end,
                nest.armed.get()
            );
        }
        return;
    }
    let gen = nest.gen.get();
    if gen >= GEN_CAP {
        if super::mutants::active("pol_no_giveup") {
            nest.fail.set(i32::MAX); // parks the countdown, keeps the region (and its failing stubs)
        } else {
            give_up(st, p, jpp, data, nest, "respecialisation cap reached");
        }
        return;
    }
    // intrinsic demotion: the slow path of an inline `math` call fired the countdown (`pc` is the CALL), or the stub after
    // an intrinsic call fails on its result guard (a replaced `math.sqrt` that returns another type): the site is
    // recompiled as an ordinary CALL; this counts as a respecialisation (bounded by the gen cap)
    {
        let sites = data.intr_sites();
        let mut hit: Vec<u32> = Vec::new();
        if sites.iter().any(|&(p, _)| p == pc) {
            hit.push(pc);
        }
        let site_info = nest.fb_mut().sites.iter().find(|s| s.pc == pc).cloned();
        for g in site_info.iter().flat_map(|s| s.guards.iter()) {
            if let Some(dpc) = g.def_pc {
                if !g.peel
                    && sites.iter().any(|&(p, _)| p == dpc)
                    && st
                        .get(rn_core::state::StkId(
                            ci.r().func.0 + 1 + usize::from(g.slot),
                        ))
                        .tag()
                        != g.tag
                {
                    hit.push(dpc);
                }
            }
        }
        if !hit.is_empty() && !super::mutants::active("pol_no_demote") {
            let mut pushed: Vec<u32> = Vec::new();
            for q in hit {
                if rt::verbose() {
                    eprintln!(
                        "[t15] nest [{}, {}] gen {gen}: intrinsic call at pc {q} demoted to an ordinary CALL",
                        nest.head, nest.end
                    );
                }
                data.intr_demoted.borrow_mut().push(q);
                pushed.push(q);
            }
            nest.gen.set(gen + 1);
            nest.entry_fail.set(0);
            nest.entry_ok.set(0);
            rearm(hot.saturating_mul(1 << (gen + 1)).min(1 << 24));
            d.stats.t15_demotions += 1;
            let plans = promoted_plans(data);
            match rt::recompile(st, p, jpp, &plans) {
                Ok(()) => {
                    // SAFETY: the per-state data is live.
                    unsafe { &mut *rt::data_ptr(st) }.stats.t15_respec += 1;
                }
                Err(e) => {
                    // the old code (with its inline intrinsics) keeps running: the demotion takes effect only when the
                    // recompile succeeds, else the site would vanish from `intr_sites()` while its inline code stays
                    if !super::mutants::active("pol_demote_no_rollback") {
                        data.intr_demoted
                            .borrow_mut()
                            .retain(|x| !pushed.contains(x));
                    }
                    if e == Fail::Retry {
                        nest.gen.set(gen);
                        rearm(hot.saturating_mul(1 << gen).min(1 << 24));
                    } else {
                        give_up(st, p, jpp, data, nest, "no compile slot left");
                    }
                }
            }
            return;
        }
    }
    // exit feedback: the tags the frame holds now, for the guards of the failing stub that fail
    let func = ci.r().func.0;
    let site = nest.fb_mut().sites.iter().find(|s| s.pc == pc).cloned();
    let mut seen: Vec<(StubGuard, u8)> = Vec::new();
    for g in site.iter().flat_map(|s| s.guards.iter()) {
        if g.peel {
            continue;
        }
        let tag = st
            .get(rn_core::state::StkId(func + 1 + usize::from(g.slot)))
            .tag();
        if super::TagSet::of_tag(tag) != super::TagSet::of_tag(g.tag) {
            seen.push((*g, tag));
        }
    }
    // no progress: nothing fails by tag class (a variant-only mismatch such as short vs long string) or a guard that
    // feedback cannot change (LEN / POW results): another round would compile the same speculation again
    if seen.is_empty() || seen.iter().any(|(g, _)| g.fixed) {
        give_up(
            st,
            p,
            jpp,
            data,
            nest,
            "the failing guard cannot be changed by feedback",
        );
        return;
    }
    // a boolean flip (`false` <-> `true`, the marks of queens' `cols[c]`) is never a one-time change: widen at once, and
    // for every load of the nest that has seen a boolean (they flip alike; one decision per load would spend the whole
    // respecialisation budget)
    let bool_flip = seen.iter().any(|(g, tag)| {
        g.def_pc.is_some()
            && super::TagSet::of_tag(*tag).subset(super::TagSet::BOOL)
            && super::TagSet::of_tag(g.tag).subset(super::TagSet::BOOL)
    }) && !super::mutants::active("pol_no_bool_widen");
    if bool_flip {
        for pc in nest.head..=nest.end {
            let b = data.prof.get(pc as usize).map_or(0xFF, Cell::get);
            if b != 0xFF && super::TagSet::of_tag(b).subset(super::TagSet::BOOL) {
                join_widen(&mut nest.fb_mut().def_widen, pc, super::TagSet::BOOL);
            }
        }
    }
    let mode = if gen == 0 && !bool_flip {
        "retarget"
    } else {
        "widen"
    };
    for &(g, tag) in &seen {
        let (obs_set, exp_set) = (super::TagSet::of_tag(tag), super::TagSet::of_tag(g.tag));
        if gen == 0 && !bool_flip {
            match g.def_pc {
                Some(dpc) => {
                    if let Some(c) = data.prof.get(dpc as usize) {
                        c.set(tag);
                    }
                }
                None => {
                    if let Some(o) = nest.obs.borrow_mut().get_mut(usize::from(g.slot)) {
                        *o = tag;
                    }
                }
            }
        } else if !super::mutants::active("pol_no_widen") {
            match g.def_pc {
                Some(dpc) => join_widen(&mut nest.fb_mut().def_widen, dpc, obs_set.join(exp_set)),
                None => join_widen(
                    &mut nest.fb_mut().entry_widen,
                    g.slot,
                    obs_set.join(exp_set),
                ),
            }
        }
        if rt::verbose() {
            eprintln!(
                "[t15] nest [{}, {}] gen {gen}: slot {} expected {:?}, saw {:?} ({}{})",
                nest.head,
                nest.end,
                g.slot,
                exp_set,
                obs_set,
                mode,
                g.def_pc
                    .map_or(String::new(), |q| format!(", load at pc {q}"))
            );
        }
    }
    nest.gen.set(gen + 1);
    nest.entry_fail.set(0);
    nest.entry_ok.set(0);
    rearm(hot.saturating_mul(1 << (gen + 1)).min(1 << 24));
    let plans = promoted_plans(data);
    match rt::recompile(st, p, jpp, &plans) {
        Ok(()) => {
            // SAFETY: the per-state data is live.
            let d = unsafe { &mut *rt::data_ptr(st) };
            d.stats.t15_respec += 1;
            if rt::verbose() {
                eprintln!(
                    "[t15] respecialised nest [{}, {}] of proto@{:x}: gen {}",
                    nest.head,
                    nest.end,
                    p.code_ptr() as usize,
                    gen + 1
                );
            }
        }
        Err(Fail::Retry) => {
            // transient (memory limit / code cap): keep the current code and decide again after the same number of
            // failures
            rearm(hot.saturating_mul(1 << gen).min(1 << 24));
            if rt::verbose() {
                eprintln!(
                    "[t15] nest [{}, {}]: respecialisation deferred (transient refusal)",
                    nest.head, nest.end
                );
            }
        }
        Err(Fail::Permanent) => {
            // the normal compile budget is spent: a nest that keeps failing must still be removable, which may use the
            // reserved slots
            if super::mutants::active("pol_cap_no_giveup") {
                nest.gen.set(GEN_CAP);
                nest.fail.set(i32::MAX); // MUTANT: the failing region stays for good
            } else {
                give_up(
                    st,
                    p,
                    jpp,
                    data,
                    nest,
                    "no compile slot left for a respecialisation",
                );
            }
        }
    }
}

fn join_widen<K: Copy + PartialEq>(v: &mut Vec<(K, super::TagSet)>, key: K, set: super::TagSet) {
    match v.iter_mut().find(|(k, _)| *k == key) {
        Some((_, s)) => *s = s.join(set),
        None => v.push((key, set)),
    }
}

fn promoted_plans(data: &T15Data) -> Vec<NestPlan> {
    data.nests
        .iter()
        .enumerate()
        .filter(|(_, n)| n.phase.get() == NestPhase::Promoted)
        .map(|(id, n)| NestPlan {
            nest: id as u32,
            head: n.head,
            end: n.end,
        })
        .collect()
}

/// "Give up": the nest goes `Off` for good and the prototype is recompiled without its region (the activation that
/// triggered it moves to the new tier-1 code through the event's continuation). May use the reserved compile slots.
fn give_up(
    st: &mut State,
    p: &Gc<Proto>,
    jpp: *mut JitProto,
    data: &T15Data,
    nest: &NestState,
    why: &str,
) {
    // SAFETY: the per-state data is live.
    let d = unsafe { &mut *rt::data_ptr(st) };
    d.stats.t15_giveups += 1;
    if rt::verbose() {
        eprintln!(
            "[t15] nest [{}, {}] given up ({why}) after {} respecialisations: back to tier 1",
            nest.head,
            nest.end,
            nest.gen.get()
        );
    }
    turn_off(d, nest);
    nest.fail.set(i32::MAX);
    let plans = promoted_plans(data);
    let _ = rt::recompile_with(st, p, jpp, &plans, true);
    if let Some(id) = data.nests.iter().position(|n| std::ptr::eq(n, nest)) {
        // SAFETY: the descriptor is live (the caller holds the frame of its prototype).
        tr_close(st, unsafe { &*jpp }, id as u32, false);
    }
}

// ---- test support ----------------------------------------------------------------------------------------

/// Result of [`test_event`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventResult {
    /// The helper's return value.
    pub ret: bool,
    /// `T15Data::cont` afterwards (a block address + offset when `ret`).
    pub cont: u64,
    /// Current version id afterwards (`None`: the frame has no tier 1.5 state).
    pub cur: Option<u8>,
}

/// Calls the event helper as exit stubs of version `ver` would, on behalf of the Lua function that called the
/// running native function (tests simulate stub events with it). Safe: it only does what the helper does.
#[doc(hidden)]
pub fn test_event(
    st: &mut State,
    ver: u32,
    site: u32,
    kind: EventKind,
    target: u32,
) -> Option<EventResult> {
    let ci = st.ci().previous()?;
    let ret = event(st, ci, ver, site, kind, target);
    let (_, jpp) = descriptor(st, ci)?;
    // SAFETY: as in `event`.
    let jp = unsafe { &*jpp };
    let t = jp.t15.as_deref()?;
    Some(EventResult {
        ret,
        cont: t.data.cont.get(),
        cur: Some(t.cur),
    })
}

/// Sets the failure countdown of nest `nest` of the Lua function that called the running native function (tests).
#[doc(hidden)]
pub fn test_set_fail(st: &mut State, nest: usize, v: i32) {
    if let Some(ci) = st.ci().previous() {
        if let Some((_, jpp)) = descriptor(st, ci) {
            // SAFETY: as in `event`.
            if let Some(t) = unsafe { &*jpp }.t15.as_deref() {
                if let Some(n) = t.data.nests.get(nest) {
                    n.fail.set(v);
                }
            }
        }
    }
}

/// Tier 1.5 facts about the Lua function that called the running native function (tests).
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtoInfo {
    /// Current version id.
    pub cur: u8,
    /// Phase of every nest.
    pub phases: Vec<NestPhase>,
    /// Retired blocks.
    pub retired: usize,
    /// Address of the current block.
    pub block: usize,
    /// Addresses of the retired blocks.
    pub retired_addrs: Vec<usize>,
    /// Promote counters.
    pub ctrs: Vec<i32>,
    /// Stub-failure countdowns.
    pub fails: Vec<i32>,
    /// `prof` bytes.
    pub prof: Vec<u8>,
    /// Bytes charged for the descriptor.
    pub charged: usize,
    /// Native offset+1 table of the current version, `resume` table likewise.
    pub pcmap: Vec<u32>,
    /// Number of instructions.
    pub code_len: usize,
}

/// See [`ProtoInfo`].
#[doc(hidden)]
pub fn test_info(st: &mut State) -> Option<ProtoInfo> {
    let ci = st.ci().previous()?;
    let (p, jpp) = descriptor(st, ci)?;
    // SAFETY: as in `event`.
    let jp = unsafe { &*jpp };
    let t = jp.t15.as_deref()?;
    Some(ProtoInfo {
        cur: t.cur,
        phases: t.data.nests.iter().map(|n| n.phase.get()).collect(),
        retired: t.retired.len(),
        block: jp.block.addr(),
        retired_addrs: t.retired.iter().map(|b| b.addr()).collect(),
        ctrs: t.data.nests.iter().map(|n| n.ctr.get()).collect(),
        fails: t.data.nests.iter().map(|n| n.fail.get()).collect(),
        prof: t.data.prof.iter().map(Cell::get).collect(),
        charged: jp.charged,
        pcmap: t.pcmap.clone(),
        code_len: p.code().len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_flags_parse() {
        let o =
            OptFlags::parse("-ra, -xmm,+randra=42,+exitstorm=7,+flushall,+verify,-bogus,+randra=x");
        assert!(o.no_ra && o.no_xmm && o.flushall && o.verify);
        assert!(!o.no_hoist);
        assert_eq!(o.randra, Some(42));
        assert_eq!(o.exitstorm, Some(7));
        assert_eq!(
            o.unknown,
            vec!["-bogus".to_string(), "+randra=x".to_string()]
        );
        assert_eq!(OptFlags::parse(""), OptFlags::default());
    }

    #[test]
    fn t15_config_parse() {
        let c = T15Config::parse(None, None, None);
        assert_eq!((c.mode, c.hot), (T15Mode::Off, T15_HOT_DEFAULT));
        let c = T15Config::parse(Some("on"), Some("100"), Some("-ra"));
        assert_eq!((c.mode, c.hot, c.opt.no_ra), (T15Mode::On, 100, true));
        let c = T15Config::parse(Some("always"), Some("100"), None);
        assert_eq!((c.mode, c.hot), (T15Mode::Always, 1));
        let c = T15Config::parse(Some("bogus"), None, None);
        assert_eq!(c.mode, T15Mode::Off);
    }
}
