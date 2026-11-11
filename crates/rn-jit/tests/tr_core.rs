#![allow(clippy::undocumented_unsafe_blocks)] // test harness: the raw accesses are on buffers the test just built

//! J5 PI: the trace compiler end to end through the REAL pipeline (`rn_jit::tr::install_driver`: recorder -> passes ->
//! register allocation -> exits -> assembler -> install), on live programs (docs/JIT_TRACE.md 9.3 subset, 9.5).
//!
//! Every job runs in a child process (this binary re-executed with `TR_CORE_JOB=<spec>`), so crashes, `RLIMIT_AS`, the
//! GC-stress / stack-move environment and the per-process hooks are contained. A job is one program (`tests/tr_core/
//! progs.rs`) in one mode; the parent compares the child's captured output (`print` / `io.write`, the final status and
//! error message with traceback) with the interpreter run (JIT off) and reads the trace statistics the child prints at
//! state close (`RUSTYNAIL_JIT_VERBOSE=1` summary).
//!
//! Matrix: modes {always, on (hot 2,2), on (default thresholds, JIT `on`), trace off}, count budgets {none, 3, 17, 200},
//! options (`-fold -cse`, `-loop`, `+exitstorm=1,2,7,61`, `+randra=1,2,3,7`, `+membudget`, `+tracemove`, `+verify`,
//! `+emurun`, `+emuverify`), GC stress 1 and 2, `RUSTYNAIL_JIT_STRESS=stackmove`, trace limits, a tiny code cap. With
//! feature `t15_oracle` the instruction-count identity of 9.5 (`fetches(off) == fetches(run) + Stats::insns(run)`).
//!
//! Run: `systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 900 cargo test -p rn-jit --features
//! trace --test tr_core -- --nocapture </dev/null` (`TR_CORE_ONLY=<substr>` selects programs, `TR_CORE_V=1` prints
//! every job's statistics).
#![cfg(feature = "trace")]

#[path = "tr_core/nr_progs.rs"]
mod nr_progs;
#[path = "tr_core/progs.rs"]
mod progs;
#[path = "tr_core/sink_progs.rs"]
mod sink_progs;

use nr_progs::NR_PROGS;
use progs::{Prog, PROGS, Y_PROGS};
use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::tr::cache::{TrConfig, TrLimits, TrMode};
use rn_jit::tr::TraceOpts;
use rn_jit::{Config, Mode};
use sink_progs::SINK_PROGS;
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BEGIN: &str = "<<<TR-CORE-BEGIN>>>";
const END: &str = "<<<TR-CORE-END>>>";

/// Lua prelude of every run: captured `print` / `io.write`.
const PRELUDE: &str = "local __b = {} \
 print = function(...) local n = select('#', ...) for i = 1, n do __b[#__b+1] = tostring((select(i, ...))) if i < n then __b[#__b+1] = '\\t' end end __b[#__b+1] = '\\n' end \
 io.write = function(...) for i = 1, select('#', ...) do __b[#__b+1] = tostring((select(i, ...))) end end \
 function __result() return table.concat(__b) end ";

// ---- the child ------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct Job {
    prog: String,
    /// `off` (JIT off), `t1` (JIT always, trace off), `always`, `on2` (on, hot 2,2), `on` (JIT on, defaults).
    trace: String,
    opts: String,
    budget: Option<i32>,
    limits: String,
    code_cap: Option<usize>,
    /// Environment of the child: `RUSTYNAIL_GC_STRESS`, `RUSTYNAIL_GC_VERIFY`, `RUSTYNAIL_JIT_STRESS`.
    env: Vec<(String, String)>,
    ident: bool,
}

impl Job {
    fn new(prog: &str, trace: &str) -> Job {
        Job {
            prog: prog.into(),
            trace: trace.into(),
            ..Job::default()
        }
    }
    fn limits_s(mut self, l: &str) -> Job {
        self.limits = l.into();
        self
    }
    fn opts(mut self, o: &str) -> Job {
        self.opts = o.into();
        self
    }
    fn budget(mut self, b: i32) -> Job {
        self.budget = Some(b);
        self
    }
    fn env(mut self, k: &str, v: &str) -> Job {
        self.env.push((k.into(), v.into()));
        self
    }
    fn spec(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.prog,
            self.trace,
            self.opts,
            self.budget.map_or(String::new(), |b| b.to_string()),
            self.limits,
            self.code_cap.map_or(String::new(), |b| b.to_string()),
            u8::from(self.ident)
        )
    }
    fn tag(&self) -> String {
        format!(
            "{} [{} {} budget={} {:?}{}]",
            self.prog,
            self.trace,
            self.opts,
            self.budget.map_or("none".into(), |b| b.to_string()),
            self.env,
            if self.limits.is_empty() {
                ""
            } else {
                " limits"
            }
        )
    }
}

#[test]
fn child_entry() {
    if let Ok(spec) = std::env::var("TR_CORE_JOB") {
        child_main(&spec);
    }
}

fn find(name: &str) -> &'static Prog {
    // `@path`: an ad-hoc program read from a file (see `adhoc_file`); leaked, it lives for the process
    if let Some(path) = name.strip_prefix('@') {
        let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        return Box::leak(Box::new(Prog {
            name: Box::leak(name.to_string().into_boxed_str()),
            small: false,
            src: Box::leak(src.into_boxed_str()),
        }));
    }
    PROGS
        .iter()
        .chain(SINK_PROGS)
        .chain(NR_PROGS)
        .chain(Y_PROGS)
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no program {name}"))
}

/// The chunk that runs `src` and returns the captured output plus the final status.
fn wrap(src: &str) -> String {
    // `TR_CORE_GCMODE=gen|inc` (set by a job's environment): the collector mode of the run
    let mode = match std::env::var("TR_CORE_GCMODE").as_deref() {
        Ok("gen") => "collectgarbage('generational') ",
        Ok("inc") => "collectgarbage('incremental') ",
        _ => "",
    };
    format!(
        "{mode}{PRELUDE}local __ok, __e = xpcall(function(...)\n{src}\nend, debug.traceback)\n\
         return __result() .. 'STATUS ' .. tostring(__ok) .. ' ' .. tostring(__e) .. '\\n'"
    )
}

fn run_chunk(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    let s = st.load_buffer(src.as_bytes(), b"=prog", None);
    assert_eq!(s, Status::Ok, "load");
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = v.as_string().map_or_else(
        || format!("<non-string result {v:?}>"),
        |s| String::from_utf8_lossy(s.as_bytes()).into_owned(),
    );
    st.set_top(base);
    format!("{status:?}|{out}")
}

fn trace_cfg(job: &Job) -> TrConfig {
    let mode = if job.trace == "always" {
        TrMode::Always
    } else {
        TrMode::On
    };
    let mut cfg = TrConfig::for_mode(mode);
    if job.trace == "on2" {
        cfg.hot_loop = 2;
        cfg.hot_exit = 2;
    }
    if job.trace == "on16" {
        // (H2: hot loops at once, the default exit threshold: the inline stubs of the frameful exits run before the handler is wanted)
        cfg.hot_loop = 2;
    }
    cfg.opts = TraceOpts::parse(&job.opts).expect("options");
    // the twin programs are tiny: their side traces are shorter than the default `sidemin`
    cfg.limits.sidemin = 0;
    if !job.limits.is_empty() {
        let sidemin = if job.limits.contains("sidemin") {
            None
        } else {
            Some(0)
        };
        cfg.limits = TrLimits::parse(&job.limits).0;
        if let Some(m) = sidemin {
            cfg.limits.sidemin = m;
        }
    }
    // K3a: the struct default of every knob is the shipped value, so every job already runs the shipped configuration. `TR_CORE_PROCESS=1` is kept
    // as an alias that additionally applies `apply_process_limits` (which differs from the default only in `lockstep`: off in a process, on in the
    // tests); the start gates stay off (the programs are tiny)
    if std::env::var("TR_CORE_PROCESS").is_ok_and(|v| v == "1") {
        let named = job.limits.clone();
        cfg.apply_process_limits(Some(&named));
        if !named.contains("sidemin") {
            cfg.limits.sidemin = 0;
        }
        if !named.contains("warm") && !named.contains("wgate") {
            cfg.limits.warm = 0;
            cfg.limits.wgate = 0;
        }
    }
    // BUNDLE-S: `TR_CORE_LOOPANY=1` runs every job with the shipped process default `loopany=1` (the struct default of the tests is 0) unless the job names it
    if let Ok(v) = std::env::var("TR_CORE_LOOPANY") {
        if !job.limits.contains("loopany") {
            cfg.limits.loopany = v.parse().unwrap_or(1);
        }
    }
    // P2 bundle 1: `TR_CORE_XREGS=3` runs every job with `rbp` / `r14` allocatable (`xregs`, the struct default of the tests is 0) unless the job names it
    if let Ok(v) = std::env::var("TR_CORE_XREGS") {
        if !job.limits.contains("xregs") {
            cfg.limits.xregs = v.parse().unwrap_or(3);
        }
    }
    // R1: `TR_CORE_REARM=4` runs every job with `rearm=4` (the struct default of the tests is 0; only the `on` jobs have a production sampling tick) unless the job names it
    if let Ok(v) = std::env::var("TR_CORE_REARM") {
        if !job.limits.contains("rearm") {
            cfg.limits.rearm = v.parse().unwrap_or(4);
        }
    }
    cfg.verbose = std::env::var("TR_CORE_VERBOSE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    // BUNDLE-M: `TR_CORE_DUMPDIR=<dir>` writes the `RUSTYNAIL_JIT_DUMP` files of the run (IR, micro-ops, machine code of every trace)
    if let Ok(d) = std::env::var("TR_CORE_DUMPDIR") {
        cfg.dump = Some(d);
    }
    cfg
}

/// One state, one run of `prog`; returns the output. `trace_on`: the J5 pipeline is attached.
/// BUNDLE-M: `__citop()` = the `CallInfo.top` of the calling Lua frame relative to its base, in slots (`top - (func + 1)`; the interpreter's value is the
/// prototype's `maxstacksize`). The native-self-call push of a trace stores `top` by hand (mutant `rec_push_top_off`); nothing else observes it.
fn citop(st: &mut rn_core::state::State) -> rn_core::status::LuaResult<usize> {
    let th = rn_core::jit_hooks::thread_ptr(st);
    // SAFETY: `th` is the running thread; its `ci` is the frame of this native, `previous` the Lua frame that called it (CallInfo fields at the
    // `jit_layout::callinfo` offsets).
    let v = unsafe {
        let ci = *th.add(rn_core::jit_hooks::thread::CI).cast::<*mut u8>();
        let prev = *ci
            .add(rn_core::jit_layout::callinfo::PREVIOUS)
            .cast::<*mut u8>();
        let top = *prev.add(rn_core::jit_layout::callinfo::TOP).cast::<usize>();
        let func = *prev
            .add(rn_core::jit_layout::callinfo::FUNC)
            .cast::<usize>();
        (top - func) as i64 - 1
    };
    st.push(rn_core::value::Value::int(v));
    Ok(1)
}

fn one_state(job: &Job, baseline: bool) -> (String, u64, u64) {
    // BUNDLE-M: `TR_CORE_SEED=<n>` pins the string-hash seed (the identity test compares dumps, whose node indices follow it)
    let mut st = match std::env::var("TR_CORE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        Some(seed) => {
            let mut st = rn_std::new_state(seed).expect("state");
            rn_std::open_libs(&mut st).expect("libs");
            st
        }
        None => rn_std::new_state_with_libs().expect("state"),
    };
    rn_jit::set_t15(
        &mut st,
        rn_jit::t15::runtime::T15Config::for_mode(rn_jit::t15::runtime::T15Mode::Off),
    );
    {
        let g = st.globals();
        st.raw_set_str(g, b"__citop", rn_core::value::Value::native(citop))
            .unwrap();
    }
    if let Some(c) = job.code_cap {
        rn_jit::set_code_cap(&mut st, c);
    }
    let traced = !baseline && matches!(job.trace.as_str(), "always" | "on2" | "on" | "on16");
    if baseline || job.trace == "off" {
        rn_jit::set_mode(&mut st, Mode::Off);
    } else {
        rn_jit::set_mode(
            &mut st,
            if job.trace == "on" {
                Mode::On
            } else {
                Mode::Always
            },
        );
        if traced {
            rn_jit::rt::set_trace(&mut st, trace_cfg(job));
            rn_jit::tr::attach_driver(&mut st);
        } else {
            // (BUNDLE-Q: a tier-1-only job may name limits; the knobs of tier 1 itself (`t1q`) apply when named)
            let mut c = TrConfig::for_mode(TrMode::Off);
            if !job.limits.is_empty() {
                c.limits = TrLimits::parse(&job.limits).0;
            }
            rn_jit::rt::set_trace(&mut st, c);
        }
        if let Some(b) = job.budget {
            rn_jit::set_count_budget(&mut st, b);
        }
    }
    let out = run_chunk(&mut st, &wrap(find(&job.prog).src));
    let stats = rn_jit::stats(&mut st);
    let iters = if traced && job.opts.contains("+verify") {
        // SAFETY: the exit area follows the budget word of the live per-state data.
        unsafe {
            (*rn_jit::rt::trace_budget_ptr(&mut st)
                .cast::<u8>()
                .add(rn_jit::tr::exit_area::FROM_R15 as usize)
                .cast::<rn_jit::tr::ExitArea>())
            .spill[rn_jit::tr::asm::ITER_SLOT as usize]
        }
    } else {
        0
    };
    drop(st); // prints the `[trace]` summary on stderr
    (out, stats.insns, iters)
}

#[cfg(feature = "t15_oracle")]
mod oracle {
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static FETCHES: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        FETCHES.store(0, Ordering::SeqCst);
        rn_core::oracle::reset_unfetched();
    }

    /// The oracle hook: counts fetches (the unfetched `TFORCALL` / `TFORLOOP` chains are counted by the interpreter
    /// itself when it executes them, `rn_core::oracle::unfetched`, Q5).
    pub fn hook(
        _frame: usize,
        _p: *const rn_core::func::Proto,
        _pc: usize,
        _base: *mut rn_core::value::Value,
        _ms: usize,
    ) {
        FETCHES.fetch_add(1, Ordering::Relaxed);
    }

    pub fn total() -> u64 {
        FETCHES.load(Ordering::SeqCst) + rn_core::oracle::unfetched()
    }
}

fn child_main(spec: &str) {
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
    let f: Vec<&str> = spec.split('|').collect();
    let job = Job {
        prog: f[0].into(),
        trace: f[1].into(),
        opts: f[2].into(),
        budget: f[3].parse().ok(),
        limits: f[4].into(),
        code_cap: f[5].parse().ok(),
        ident: f[6] == "1",
        env: Vec::new(),
    };
    let cfg = Config::for_mode(if job.trace == "on" {
        Mode::On
    } else {
        Mode::Always
    });
    assert!(rn_jit::register_with_trace_pinned_off(cfg), "register");
    rn_jit::tr::install_driver();
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
    if let Some(n) = std::env::var("TR_CORE_MATFAULT")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        rn_jit::tr::exit::set_materialise_fault(n);
    }
    if job.ident {
        #[cfg(feature = "t15_oracle")]
        {
            rn_core::oracle::set_hook(Some(oracle::hook));
            oracle::reset();
            let (o0, i0, _) = one_state(&job, true);
            let f0 = oracle::total();
            oracle::reset();
            let (o1, insns, _) = one_state(&job, false);
            let f1 = oracle::total();
            rn_core::oracle::set_hook(None);
            println!(
                "{BEGIN}\nIDENT off={f0} fetches={f1} insns={insns} baseline_insns={i0} same_output={}\n{END}",
                o0 == o1
            );
            return;
        }
        #[cfg(not(feature = "t15_oracle"))]
        panic!("ident jobs need feature t15_oracle");
    }
    let (out, insns, iters) = one_state(&job, false);
    println!(
        "{BEGIN}\n{out}\n@@insns={insns} iterations={iters} verified={}\n{END}",
        rn_jit::tr::emuverify_count()
    );
}

// ---- the parent -----------------------------------------------------------------------------------------------------

/// What the child's `[trace]` summary said.
#[derive(Clone, Debug, Default)]
struct TStats {
    /// S0: tables sunk by the compiles of the run.
    sunk: u64,
    /// J1-2: calls of the materialising helper by inline exit stubs.
    stub_calls: u64,
    /// Tables materialised by exits (`sink: ... materialised N`).
    materialised: u64,
    stub_frames: u64,
    /// BUNDLE-1: loops peeled by the compiles of the run.
    peeled: u64,
    /// BUNDLE-N: installed traces with a `RETF`.
    retf: u64,
    recorded: u64,
    installed: u64,
    entered: u64,
    flushes: u64,
    rerecord: u64,
    blacklisted: u64,
    refused: u64,
    ineffective: u64,
    hot_type: u64,
    hot_other: u64,
    exits: u64,
    aborts: BTreeMap<String, u64>,
    /// F2a: side recordings started / side traces installed / failed.
    side_recorded: u64,
    side_installed: u64,
    side_failed: u64,
    side_preludes: u64,
    /// H3-A: handler calls for blacklisted exits.
    black_handled: u64,
    /// BUNDLE-J: prototypes registered as C-entered / of those with a light root / calls from C that entered compiled code.
    c_registered: u64,
    c_light: u64,
    c_entries: u64,
    /// J1-1: fused HREFK+HLOAD pairs of the installed traces.
    hk_fused: u64,
    /// J1-1: of those, with two exits.
    hk_split: u64,
    side_aborts: BTreeMap<String, u64>,
    /// F2b: nest roots installed / traces with a linking exit.
    nest_roots: u64,
    frame_links: u64,
    /// C38: side traces that end with a frame-linking exit.
    side_links: u64,
    stitches: u64,
    /// J2-2: sites re-recorded deeper / deep recordings started.
    deep_sites: u64,
    deep_recs: u64,
    /// BUNDLE-S/P: `loopany` guard firings / whole-family flushes / pruned side traces / of those the trace whose exit raised the guard.
    la_fired: u64,
    la_family: u64,
    la_pruned: u64,
    la_self: u64,
    /// C36: entry failures repaired by `cispare` / roots re-recorded by `domtop` / heads given a second chance / identical re-records installed by `second`.
    ci_repaired: u64,
    dom_flushes: u64,
    /// P3: chain re-orders / checks (`chainre`), heads re-recorded without entry callee guards (`ncallee`).
    chain_flushes: u64,
    chain_checks: u64,
    /// BUNDLE-G: callee-guard exits that got no further link (`chaindepth`).
    chain_deep: u64,
    ncallee_heads: u64,
    retried: u64,
    ident_kept: u64,
    /// C39: retry rounds started for heads blacklisted by `InnerLoop` aborts (`innerretry`).
    inner_retried: u64,
    /// BUNDLE-C: function-entry heads promoted by `fnpromo`.
    fn_promoted: u64,
    /// R1: rearm ticks armed / entering a traced head / without one.
    rearm_arms: u64,
    rearm_enters: u64,
    rearm_misses: u64,
    rearm_restored: u64,
    /// Y5: calls of the barrier helper (`jit_tr_barrier`): back barrier of a table store / forward barrier of an upvalue store.
    barrier_table: u64,
    barrier_upvalue: u64,
    /// C45: young-head refusals by the clean test: (failed side recording, unrolled callee loop).
    rearm_unclean: (u64, u64),
}

impl TStats {
    fn parse(err: &str) -> TStats {
        let mut s = TStats::default();
        for l in err.lines().filter_map(|l| l.strip_prefix("[trace] ")) {
            if let Some(r) = l.strip_prefix("recorded ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                let g = |k: &str| {
                    w.iter()
                        .position(|x| *x == k)
                        .and_then(|i| w.get(i + 1))
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0)
                };
                s.recorded = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.installed = g("installed");
                s.entered = g("entered");
                s.flushes = g("flushes");
                s.rerecord = g("rerecord");
                s.blacklisted = g("blacklisted");
                s.refused = g("refused");
                s.ineffective = g("ineffective");
            } else if let Some(r) = l.strip_prefix("hot exits: type-guard ") {
                let (a, b) = r.split_once(" other ").unwrap_or(("0", "0"));
                s.hot_type = a.trim().parse().unwrap_or(0);
                s.hot_other = b
                    .split(';')
                    .next()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("side traces: recorded ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                let g = |k: &str| {
                    w.iter()
                        .position(|x| *x == k)
                        .and_then(|i| w.get(i + 1))
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0)
                };
                s.side_recorded = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.side_installed = g("installed");
                s.side_failed = g("failed");
                s.side_preludes = g("preludes");
            } else if let Some(r) = l.strip_prefix("centry: registered ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                s.c_registered = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.c_light = w.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
                s.c_entries = w.get(4).and_then(|v| v.parse().ok()).unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("exit handler: blacklisted exits reported ") {
                s.black_handled = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("fused HREFK+HLOAD tag checks: ") {
                s.hk_fused = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.hk_split = r
                    .split("two exits: ")
                    .nth(1)
                    .and_then(|v| v.trim_end_matches(')').parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("loopany guard: fired ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                let g = |k: &str| {
                    w.iter()
                        .position(|x| *x == k)
                        .and_then(|i| w.get(i + 1))
                        .and_then(|v| v.trim_end_matches(')').parse::<u64>().ok())
                        .unwrap_or(0)
                };
                s.la_fired = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.la_family = g("flushes");
                s.la_pruned = g("sides");
                s.la_self = g("(self");
            } else if let Some(r) = l.strip_prefix("cispare: entry failures repaired ") {
                s.ci_repaired = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("domtop: roots re-recorded ") {
                s.dom_flushes = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("chainre: callee chains re-ordered ") {
                s.chain_flushes = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.chain_checks = r
                    .split("checks ")
                    .nth(1)
                    .and_then(|v| v.trim_end_matches(')').trim().parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) =
                l.strip_prefix("chaindepth: callee-guard exits that got no further link ")
            {
                s.chain_deep = r.trim().parse().unwrap_or(0);
            } else if let Some(r) =
                l.strip_prefix("ncallee: heads re-recorded without entry callee guards ")
            {
                s.ncallee_heads = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("barrier helper calls: table ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                s.barrier_table = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.barrier_upvalue = w.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("rearm: armed ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                s.rearm_arms = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.rearm_enters = w.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
                s.rearm_misses = w.get(4).and_then(|v| v.parse().ok()).unwrap_or(0);
                s.rearm_restored = w.get(6).and_then(|v| v.parse().ok()).unwrap_or(0);
                if let Some((a, b)) = w.get(8).and_then(|v| v.split_once('/')) {
                    s.rearm_unclean = (a.parse().unwrap_or(0), b.parse().unwrap_or(0));
                }
            } else if let Some(r) = l.strip_prefix("fnpromo: function heads promoted ") {
                s.fn_promoted = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("innerretry: rounds started ") {
                s.inner_retried = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("second: heads given a second chance ") {
                s.retried = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.ident_kept = r
                    .split("installed ")
                    .nth(1)
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("deep sites: ") {
                s.deep_sites = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.deep_recs = r
                    .split("recordings: ")
                    .nth(1)
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("links: nest roots ") {
                let w: Vec<&str> = r.split_whitespace().collect();
                let g = |k: &str| {
                    w.iter()
                        .position(|x| *x == k)
                        .and_then(|i| w.get(i + 1))
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0)
                };
                s.nest_roots = w.first().and_then(|v| v.parse().ok()).unwrap_or(0);
                s.frame_links = g("links");
                s.stitches = g("stitches");
                s.side_links = g("sidelinks");
            } else if let Some(r) = l.strip_prefix("side aborted ") {
                if let Some((k, v)) = r.rsplit_once(": ") {
                    s.side_aborts
                        .insert(k.into(), v.trim().parse().unwrap_or(0));
                }
            } else if let Some(r) = l.strip_prefix("aborted ") {
                if let Some((k, v)) = r.rsplit_once(": ") {
                    s.aborts.insert(k.into(), v.trim().parse().unwrap_or(0));
                }
            } else if let Some(r) = l.strip_prefix("sink: tables ") {
                s.retf = r
                    .split(" retf ")
                    .nth(1)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.peeled = r
                    .split("loops peeled ")
                    .nth(1)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.sunk = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.materialised = r
                    .split("materialised ")
                    .nth(1)
                    .and_then(|v| v.split(|c: char| !c.is_ascii_digit()).next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.stub_frames = r
                    .split("stub-frames ")
                    .nth(1)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                s.stub_calls = r
                    .split("stub-calls ")
                    .nth(1)
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = l.strip_prefix("exits ") {
                s.exits = r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        }
        s
    }

    fn aborted(&self) -> u64 {
        self.aborts.values().sum()
    }

    fn line(&self) -> String {
        format!(
            "deep {}/{} recorded {} installed {} entered {} exits {} flushes {} rerec {} black {} ineff {} hot(type/other) {}/{} aborts {:?} side(rec/inst/fail) {}/{}/{} {:?} sunk {} stub-calls {} stub-frames {}",
            self.deep_sites,
            self.deep_recs,
            self.recorded,
            self.installed,
            self.entered,
            self.exits,
            self.flushes,
            self.rerecord,
            self.blacklisted,
            self.ineffective,
            self.hot_type,
            self.hot_other,
            self.aborts,
            self.side_recorded,
            self.side_installed,
            self.side_failed,
            self.side_aborts,
            self.sunk,
            self.stub_calls,
            self.stub_frames
        )
    }
}

#[derive(Clone, Debug)]
struct Out {
    ok: bool,
    result: String,
    stderr: String,
    stats: TStats,
    insns: u64,
    iterations: u64,
    verified: u64,
}

fn run_job(job: &Job) -> Out {
    let exe = std::env::current_exe().expect("exe");
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env("TR_CORE_JOB", job.spec())
        .env_remove("RUSTYNAIL_JIT")
        .env_remove("RUSTYNAIL_JIT_TRACE")
        .env_remove("RUSTYNAIL_GC_STRESS")
        .env_remove("RUSTYNAIL_JIT_STRESS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &job.env {
        cmd.env(k, v);
    }
    // `TR_CORE_MEM_MB=n`: caps the address space of the job's process (RLIMIT_AS; a mutated pipeline can allocate
    // without bound, which got the mutant runners OOM-killed); 0 or unset: no cap
    // (default for the jobs of a mutant run: 2048)
    let mutated = job.env.iter().any(|(k, _)| k == "RUSTYNAIL_JIT_MUT");
    if let Some(mb) = std::env::var("TR_CORE_MEM_MB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .or(mutated.then_some(2048))
        .filter(|&m| m > 0)
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(move || {
                let l = libc::rlimit {
                    rlim_cur: mb << 20,
                    rlim_max: mb << 20,
                };
                libc::setrlimit(libc::RLIMIT_AS, &l);
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn().expect("spawn");
    let mut so = child.stdout.take().unwrap();
    let mut se = child.stderr.take().unwrap();
    let h1 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let h2 = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let t0 = Instant::now();
    // a mutated pipeline may hang a child: those jobs get a short leash
    let default = if job.env.iter().any(|(k, _)| k == "RUSTYNAIL_JIT_MUT") {
        20
    } else {
        120
    };
    let limit = Duration::from_secs(
        std::env::var("TR_CORE_TIMEOUT").map_or(default, |v| v.parse().unwrap_or(default)),
    );
    let status = loop {
        match child.try_wait().expect("wait") {
            Some(s) => break Some(s),
            None if t0.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let stdout = h1.join().unwrap();
    let stderr = h2.join().unwrap();
    let body = stdout
        .split_once(BEGIN)
        .and_then(|(_, r)| r.split_once(END))
        .map(|(b, _)| b.trim_start_matches('\n').to_string());
    let ok = status.is_some_and(|s| s.success()) && body.is_some();
    let body = body.unwrap_or_else(|| format!("<no result: status {status:?}>\n{stdout}"));
    let (result, insns, iterations, verified) = match body.rsplit_once("\n@@insns=") {
        Some((r, t)) => {
            let mut it = t.split_whitespace();
            let a = it.next().and_then(|v| v.parse().ok()).unwrap_or_default();
            let mut num = |k: &str| {
                it.next()
                    .and_then(|v| v.strip_prefix(k))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_default()
            };
            let b = num("iterations=");
            let c = num("verified=");
            (r.to_string(), a, b, c)
        }
        None => (body, 0, 0, 0),
    };
    let stats = TStats::parse(&stderr);
    Out {
        ok,
        result,
        stderr,
        stats,
        insns,
        iterations,
        verified,
    }
}

/// Runs the jobs on a few worker threads.
fn run_all(jobs: Vec<Job>) -> Vec<(Job, Out)> {
    // `TR_CORE_JOBS` caps the worker threads (each runs one child process); several tests run at once, so the
    // default stays small to keep a full run from oversubscribing the machine.
    let n = std::env::var("TR_CORE_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(3, |n| n.get())
                .min(3)
        })
        .max(1);
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| loop {
                let next = queue.lock().unwrap().pop();
                let Some((i, j)) = next else { break };
                let o = run_job(&j);
                results.lock().unwrap().push((i, j, o));
            });
        }
    });
    let mut v = results.into_inner().unwrap();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|(_, j, o)| (j, o)).collect()
}

fn selected(small_only: bool) -> Vec<&'static Prog> {
    let only = std::env::var("TR_CORE_ONLY").ok();
    PROGS
        .iter()
        .chain(SINK_PROGS)
        .chain(NR_PROGS)
        .filter(|p| !small_only || p.small)
        .filter(|p| only.as_ref().is_none_or(|o| p.name.contains(o.as_str())))
        .collect()
}

fn verbose() -> bool {
    std::env::var("TR_CORE_V").is_ok()
}

/// Runs `jobs` and compares each result with the interpreter baseline of its program; `Err` lists the divergences.
fn compare_try(jobs: Vec<Job>) -> Result<Vec<(Job, Out)>, String> {
    let mut base = BTreeMap::new();
    let progs: Vec<String> = {
        let mut v: Vec<String> = jobs.iter().map(|j| j.prog.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    for (j, o) in run_all(progs.iter().map(|p| Job::new(p, "off")).collect()) {
        assert!(
            o.ok,
            "baseline {} failed:\n{}\n{}",
            j.prog, o.result, o.stderr
        );
        assert!(
            o.result.contains("STATUS true"),
            "baseline {} did not finish: {}",
            j.prog,
            o.result
        );
        base.insert(j.prog.clone(), o.result);
    }
    let res = run_all(jobs);
    let mut bad = Vec::new();
    for (j, o) in &res {
        if verbose() {
            eprintln!("{:<60} {}", j.tag(), o.stats.line());
        }
        if j.ident {
            if let Err(e) = ident_check(o) {
                bad.push(format!("{}: identity: {e}", j.tag()));
            }
        } else if !o.ok {
            bad.push(format!(
                "{}: child failed\n{}\n{}",
                j.tag(),
                o.result,
                tail(&o.stderr)
            ));
        } else if o.result != base[&j.prog] {
            bad.push(format!(
                "{}: output differs\n--- interpreter\n{}\n--- run\n{}\n{}",
                j.tag(),
                base[&j.prog],
                o.result,
                tail(&o.stderr)
            ));
        }
    }
    if bad.is_empty() {
        Ok(res)
    } else {
        Err(format!(
            "{} failure(s):\n{}",
            bad.len(),
            bad.join("\n=====\n")
        ))
    }
}

fn compare(jobs: Vec<Job>) -> Vec<(Job, Out)> {
    compare_try(jobs).unwrap_or_else(|e| panic!("{e}"))
}

/// The 9.5 identity of an `ident` job: `fetches(off) == fetches(run) + insns(run)` and the same output.
fn ident_check(o: &Out) -> Result<String, String> {
    let line = o
        .result
        .lines()
        .find(|l| l.starts_with("IDENT"))
        .unwrap_or("");
    let num = |k: &str| -> i64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(k))
            .and_then(|v| v.parse().ok())
            .unwrap_or(-1)
    };
    let (f0, f1, insns) = (num("off="), num("fetches="), num("insns="));
    if !o.ok || f0 < 0 || f0 != f1 + insns || !line.contains("same_output=true") {
        Err(format!("{line} {}", tail(&o.stderr)))
    } else {
        Ok(format!("off={f0} fetches={f1} insns={insns}"))
    }
}

fn tail(s: &str) -> String {
    let v: Vec<&str> = s.lines().collect();
    v[v.len().saturating_sub(25)..].join("\n")
}

fn report(title: &str, res: &[(Job, Out)]) {
    eprintln!("--- {title}");
    for (j, o) in res {
        eprintln!("{:<24} insns {:<8} {}", j.prog, o.insns, o.stats.line());
    }
}

// ---- the tests ------------------------------------------------------------------------------------------------------

/// Programs whose loops the prototype records and runs natively in `always` mode.
const MUST_TRACE: &[&str] = &[
    "int_sum",
    "float_poly",
    "array_rec",
    "int_rec2",
    "inline_leaf",
    "inline_2",
    "upval_acc",
    "intrinsics",
    "type_entry",
    "type_late",
    "nested_tables",
    "float_forloop",
    "while_repeat",
];

#[test]
fn twins_always_and_on() {
    let progs = selected(false);
    let mut jobs = Vec::new();
    for p in &progs {
        jobs.push(Job::new(p.name, "always"));
        jobs.push(Job::new(p.name, "on2"));
        jobs.push(Job::new(p.name, "on"));
        jobs.push(Job::new(p.name, "t1"));
    }
    let res = compare(jobs);
    report(
        "always (per-program trace statistics)",
        &res.iter()
            .filter(|(j, _)| j.trace == "always")
            .cloned()
            .collect::<Vec<_>>(),
    );
    let all = std::env::var("TR_CORE_ONLY").is_err();
    for (j, o) in &res {
        if j.trace == "t1" {
            assert_eq!(
                o.stats.installed,
                0,
                "{}: trace off must install nothing",
                j.tag()
            );
            assert_eq!(
                o.stats.recorded,
                0,
                "{}: trace off must record nothing",
                j.tag()
            );
        }
        if all && j.trace == "always" && MUST_TRACE.contains(&j.prog.as_str()) {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.tag(),
                o.stats.line()
            );
            assert!(
                o.stats.entered >= 1,
                "{}: trace never entered: {}",
                j.tag(),
                o.stats.line()
            );
        }
    }
}

#[test]
fn twins_budgets() {
    let mut jobs = Vec::new();
    for p in selected(false) {
        for b in [3, 17, 200] {
            jobs.push(Job::new(p.name, "always").budget(b));
            jobs.push(Job::new(p.name, "on2").budget(b));
        }
    }
    compare(jobs);
}

#[test]
fn twins_options() {
    let variants = [
        "-fold -cse",
        "-fold",
        "-cse",
        "-loop -fwd -abc -sink",
        "-loop",
        "-fwd",
        "-loop -fold",
        "-cse -loop",
        "-fwd +exitstorm=3",
        "-loop +exitstorm=2",
        "+exitstorm=1",
        "+exitstorm=2",
        "+exitstorm=7",
        "+exitstorm=61",
        "+randra=1",
        "+randra=2",
        "+randra=3",
        "+randra=7",
        "+membudget",
        "+tracemove",
        "+verify",
        "+verify +randra=5 +exitstorm=5",
        "+verify -fwd +randra=6",
        "+randra=8 -loop",
    ];
    let mut jobs = Vec::new();
    for p in selected(false) {
        for v in variants {
            let mut j = Job::new(p.name, "always").opts(v);
            // K3a: `+tracemove` copies the whole Lua stack at every trace entry; with the shipped native recursion every level of a recursion to the
            // stack limit (`pm3_rec`, `bf_rec_deep`, `m1_interp`: ~200000 levels) is an entry, so the run is quadratic and hits the 120 s leash.
            // Those programs run the option without native recursion (the stack-move stress of native recursion is `twins_stackmove`).
            if v == "+tracemove"
                && ["pm3_rec", "bf_rec_deep", "m1_interp", "bk_obs2"].contains(&p.name)
            {
                j = j.limits_s("recnat=0,recany=0");
            }
            jobs.push(j);
        }
        jobs.push(Job::new(p.name, "on2").opts("+exitstorm=3 +randra=9"));
    }
    compare(jobs);
}

#[test]
fn twins_emulator_modes() {
    let mut jobs = Vec::new();
    for p in selected(false) {
        jobs.push(Job::new(p.name, "always").opts("+emurun"));
        jobs.push(Job::new(p.name, "always").opts("+emurun +exitstorm=7"));
        jobs.push(Job::new(p.name, "always").opts("+emuverify"));
        jobs.push(Job::new(p.name, "always").opts("+emuverify +exitstorm=5 +membudget"));
    }
    let res = compare(jobs);
    if std::env::var("TR_CORE_ONLY").is_err() {
        let emu: u64 = res
            .iter()
            .filter(|(j, _)| j.opts.contains("+emu"))
            .map(|(_, o)| o.stats.installed)
            .sum();
        assert!(emu > 20, "the emulator modes ran {emu} traces only");
        let verified: u64 = res
            .iter()
            .filter(|(j, _)| j.opts.contains("+emuverify"))
            .map(|(_, o)| o.verified)
            .sum();
        assert!(verified > 1000, "+emuverify compared only {verified} exits");
    }
}

/// PIb: `+emuverify` across the exit kinds that P5c added: generic entry-stub exits (`exitno` tagged with
/// `EXIT_ENTRY`: `type_entry*`, `type_flap` re-enter heads with changed types), rolled-layout first-iteration exits
/// (the extension-slot flag: `inline_exit*`, `tc_livein`, `nested_tables`), and hot diverts (`type_late`, `type_flap`,
/// `+exitstorm`). Without the decoding of the entry flag the verify probe indexed `meta.exits` out of bounds.
#[test]
fn emuverify_entry_rolled_hot() {
    let progs = [
        "type_entry",
        "type_flap",
        "type_late",
        "tc_livein",
        "inline_exit",
        "inline_exit_k",
        "nested_tables",
        "int_sum",
    ];
    let mut jobs = Vec::new();
    for p in progs {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+emuverify"));
            jobs.push(Job::new(p, trace).opts("+emuverify +exitstorm=3 +membudget"));
            jobs.push(Job::new(p, trace).opts("+emuverify").budget(50));
        }
    }
    let res = compare(jobs);
    let verified: u64 = res.iter().map(|(_, o)| o.verified).sum();
    assert!(verified > 500, "+emuverify compared only {verified} exits");
}

#[test]
fn twins_gc_stress() {
    let mut jobs = Vec::new();
    for p in selected(true) {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p.name, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p.name, "on2")
                    .opts("+exitstorm=7")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
    }
    // collector verification on the smallest ones
    for name in ["int_sum", "collectable_stores", "gc_in_loop", "intrinsics"] {
        if selected(true).iter().any(|p| p.name == name) {
            jobs.push(
                Job::new(name, "always")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1"),
            );
        }
    }
    compare(jobs);
}

#[test]
fn twins_stackmove() {
    let mut jobs = Vec::new();
    // (`pm3_rec`, `r2_rec_deep` and `bf_rec_deep` recurse to the stack limit: quadratic when the stack moves at every helper call; `pm3_rec2` and `r2_rec` cover it)
    for p in selected(false).into_iter().filter(|p| {
        p.name != "pm3_rec"
            && p.name != "r2_rec_deep"
            && p.name != "bf_rec_deep"
            && p.name != "bk_obs2"
    }) {
        jobs.push(Job::new(p.name, "always").env("RUSTYNAIL_JIT_STRESS", "stackmove"));
        jobs.push(
            Job::new(p.name, "always")
                .opts("+tracemove")
                .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
        );
    }
    compare(jobs);
}

#[test]
fn twins_limits_and_caps() {
    let mut jobs = Vec::new();
    for p in selected(false) {
        let mut j = Job::new(p.name, "always");
        j.limits = "maxtrace=1".into();
        jobs.push(j);
        let mut j = Job::new(p.name, "always");
        j.limits = "maxrecord=20,maxsnap=3".into();
        jobs.push(j);
        let mut j = Job::new(p.name, "always");
        j.code_cap = Some(64 << 10);
        jobs.push(j);
        // F2b: linking exits and stitching on, linking off
        for (t, l) in [
            ("always", "nest=2,stitchmin=4"),
            ("on2", "nest=2,stitchmin=4"),
            ("always", "nest=0"),
        ] {
            let mut j = Job::new(p.name, t);
            j.limits = l.into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        // a refusal is a counted abort, never a half-installed trace: installed + aborted covers every recording
        assert!(
            o.stats.installed + o.stats.aborted() + o.stats.refused
                >= o.stats.recorded.saturating_sub(1),
            "{}: recordings unaccounted for: {}",
            j.tag(),
            o.stats.line()
        );
    }
}

/// 3.3: the two type-stability patterns and their stable twins (VERBOSE statistics).
#[test]
fn type_stability() {
    // BUNDLE-M M2: the expectations below encode the pre-hoist trace dynamics (`bhoist` is on by default now), so they pin `bhoist=0`;
    // `type_stability_hoisted` restates the type_late expectation under hoisting.
    let run = |p: &str, trace: &str| {
        let r = run_all(vec![Job::new(p, trace).limits_s("bhoist=0")])
            .remove(0)
            .1;
        assert!(r.ok, "{p}: {}", r.stderr);
        eprintln!("{p:<20} [{trace}] {}", r.stats.line());
        r.stats
    };
    for trace in ["always", "on2"] {
        let base_black = run("type_entry_stable", trace);
        let e = run("type_entry", trace);
        // the free peel (a live-in that is an integer at every entry) is never counted: no hot type-guard exit, no
        // re-record, the same single trace as the stable twin, the inner head never Black
        assert_eq!(e.hot_type, 0, "type_entry: hot type-guard exits");
        assert_eq!(e.rerecord, 0, "type_entry: re-records");
        assert_eq!(
            e.installed, base_black.installed,
            "type_entry: traces vs twin"
        );
        assert_eq!(
            e.blacklisted, base_black.blacklisted,
            "type_entry: a head went Black"
        );
        assert!(
            e.entered >= 500,
            "type_entry: the trace is not running: {}",
            e.line()
        );
        let s = run("type_entry_stable", trace);
        // (F3: the loop that fills the table, `t[j] = ...` past the array part, is traced too: one more trace)
        assert!(
            (1..=2).contains(&(s.installed - s.nest_roots)),
            "stable twin keeps one trace (besides nest roots, F2b, and the fill loop, F3)"
        );
        assert_eq!((s.rerecord, s.hot_type), (0, 0));
        // type_late: the hot type-guard exit of the peeled root gets a side trace (F2a); the root and its side trace
        // then ping-pong, which the ineffective-trace rule (Q2) ends with exactly one re-record
        let sl = run("type_late_stable", trace);
        let l = run("type_late", trace);
        assert!(
            (1..=2).contains(&(sl.installed - sl.nest_roots)),
            "type_late_stable keeps one trace (besides the fill loop, F3)"
        );
        assert_eq!((sl.rerecord, sl.hot_type), (0, 0));
        assert_eq!(
            l.rerecord,
            1,
            "type_late: exactly one re-record: {}",
            l.line()
        );
        assert_eq!(
            l.hot_type, 0,
            "type_late: the type-guard exit gets a side trace"
        );
        assert_eq!(l.ineffective, 1, "type_late: the ping-pong is cut by Q2");
        assert!(
            l.side_installed >= 1,
            "type_late: a side trace: {}",
            l.line()
        );
        // (+ the loop that fills the table, F3)
        assert_eq!(
            l.installed - l.nest_roots,
            3,
            "type_late: the integer trace and the float trace, and the fill loop"
        );
        assert_eq!(
            l.blacklisted, sl.blacklisted,
            "type_late: a head went Black"
        );
        assert!(
            l.entered >= 300,
            "type_late: the float trace is not running: {}",
            l.line()
        );
        // a head whose type keeps flipping is re-recorded three times and then goes Black
        let f = run("type_flap", trace);
        let fs = run("type_late_stable", trace);
        // (before F2a: 4; the side traces absorb part of the flips, the head still ends Black)
        assert!(
            f.rerecord >= 2,
            "type_flap: re-records before Black: {}",
            f.line()
        );
        assert!(
            f.blacklisted > fs.blacklisted,
            "type_flap: the head goes Black: {}",
            f.line()
        );
    }
}

/// BUNDLE-M M2: `type_late` with `bhoist=1` (the default). The hoisted pre-roll guards change the dynamics: the integer root's type-guard exit is
/// handled by side traces (their recording aborts with `LeaveFrame` more often), the ping-pong never reaches the ineffective-trace rule (`rerec 0`), but
/// the intent is the same: the float phase is covered by an installed side trace or re-recorded trace, runs natively, no head goes Black, no hot type-guard exit remains.
#[test]
fn type_stability_hoisted() {
    for trace in ["always", "on2"] {
        let run = |p: &str| {
            let r = run_all(vec![Job::new(p, trace).limits_s("bhoist=1")])
                .remove(0)
                .1;
            assert!(r.ok, "{p}: {}", r.stderr);
            r.stats
        };
        let sl = run("type_late_stable");
        let l = run("type_late");
        assert_eq!((sl.rerecord, sl.hot_type), (0, 0));
        assert!(
            l.rerecord <= 1,
            "type_late (hoisted): at most one re-record: {}",
            l.line()
        );
        assert_eq!(
            l.hot_type,
            0,
            "type_late (hoisted): the type-guard exit gets a side trace: {}",
            l.line()
        );
        assert!(
            l.side_installed >= 1,
            "type_late (hoisted): a side trace: {}",
            l.line()
        );
        assert_eq!(
            l.blacklisted,
            sl.blacklisted,
            "type_late (hoisted): a head went Black: {}",
            l.line()
        );
        assert!(
            l.entered >= 300,
            "type_late (hoisted): the float trace is not running: {}",
            l.line()
        );
        assert!(
            l.installed - l.nest_roots <= 3,
            "type_late (hoisted): no trace explosion: {}",
            l.line()
        );
    }
}

/// F2b: nests. The outer loop's recording reaches the head of the inner loop, which has a root trace, and ends with a
/// link into it (`nest roots` >= 1); the same programs with `nest=0` record no nest root (mutants `nest_link_never`,
/// `side_untraced_retry`: the exits of the inner loop wait for the outer trace and then get side traces that link back).
#[test]
fn nest_links() {
    for trace in ["always", "on2"] {
        for p in ["nest_mandel", "nest_types", "nest_flush", "nest_dep"] {
            let with = run_all(vec![Job::new(p, trace).opts("+verify")])
                .remove(0)
                .1;
            assert!(with.ok, "{p} [{trace}]: {}", with.stderr);
            let mut j = Job::new(p, trace).opts("+verify");
            j.limits = "nest=0".into();
            let without = run_all(vec![j]).remove(0).1;
            assert!(without.ok, "{p} [{trace}] nest=0: {}", without.stderr);
            eprintln!(
                "{p:<14} [{trace}] {} (nest=0: {})",
                with.stats.line(),
                without.stats.line()
            );
            assert_eq!(
                without.stats.nest_roots, 0,
                "{p}: nest=0 records no nest root"
            );
            assert!(
                with.stats.nest_roots >= 1,
                "{p} [{trace}]: no nest root: {}",
                with.stats.line()
            );
            if p == "nest_mandel" {
                // the inner loop's exits continue in side traces that link to the outer trace
                assert!(
                    with.stats.side_installed >= 2,
                    "{p} [{trace}]: {}",
                    with.stats.line()
                );
            }
        }
    }
}

/// F2b: linking exits (`nest=2`: the caller's trace materialises an inlined callee's frame and continues in the callee's
/// loop trace) and stitching at C calls (`stitchmin=4`). Mutants `frame_link_never`, `frame_link_cell_ignored` (the exit
/// lands at the tier-1 label: the callee's trace is entered less often), `stitch_never`.
#[test]
fn frame_links_and_stitches() {
    for trace in ["always", "on2"] {
        for p in ["nest_frame_scan", "nest_frame_pixel"] {
            let mut j = Job::new(p, trace).opts("+verify");
            // (BUNDLE-S: `loopany=0`: with the process default a callee `while` loop is unrolled into the caller instead of linked, which is what this test is not about)
            j.limits = "nest=2,loopany=0,retf=2".into();
            let with = compare(vec![j]).remove(0).1;
            assert!(with.ok, "{p} [{trace}]: {}", with.stderr);
            let mut j = Job::new(p, trace).opts("+verify");
            j.limits = "nest=1,loopany=0,retf=2".into();
            let without = run_all(vec![j]).remove(0).1;
            assert!(without.ok, "{p} [{trace}] nest=1: {}", without.stderr);
            eprintln!(
                "{p:<18} [{trace}] {} (nest=1: {})",
                with.stats.line(),
                without.stats.line()
            );
            assert_eq!(
                without.stats.frame_links, 0,
                "{p}: nest=1 records no linking exit"
            );
            assert!(
                with.stats.frame_links >= 1,
                "{p} [{trace}]: no linking exit: {}",
                with.stats.line()
            );
            // the callee's loop trace is entered from the caller's trace (through the cell): per outer iteration the
            // caller's trace and the callee's are both entered, against the callee's alone without the link (the mutant
            // `frame_link_cell_ignored` lands in tier 1 behind the callee's head and enters only the caller's)
            assert!(
                2 * with.stats.entered >= 3 * without.stats.entered,
                "{p} [{trace}]: the linked trace is not entered through the link: {} vs {}",
                with.stats.entered,
                without.stats.entered
            );
        }
        let mut j = Job::new("nest_stitch", trace).opts("+verify");
        j.limits = "stitchmin=4,libcall=7".into(); // (BUNDLE-R: `string.rep` is recorded by default now; the program wants a C call that stitches)
        let with = compare(vec![j]).remove(0).1;
        assert!(with.ok, "nest_stitch [{trace}]: {}", with.stderr);
        eprintln!("nest_stitch [{trace}] {}", with.stats.line());
        assert!(
            with.stats.stitches >= 1,
            "nest_stitch [{trace}]: no stitching trace: {}",
            with.stats.line()
        );
        let mut j = Job::new("nest_stitch", trace).opts("+verify");
        j.limits = "stitchmin=0,libcall=7".into();
        let off = run_all(vec![j]).remove(0).1;
        assert!(off.ok, "nest_stitch [{trace}] stitchmin=0: {}", off.stderr);
        assert_eq!(off.stats.stitches, 0, "stitchmin=0 stitches nothing");
    }
}

/// F2b (acceptance): a link enters the target's entry with its guards re-checked. The inner loop of `nest_tr6d` is entered
/// from the outer trace through the head's dispatch cell while the closure it calls changes between phases; the result
/// equals the interpreter's in every mode. (The recorder rule behind TR6d and the mutant `entry_guard_past_store` are
/// tested for nest roots at record level: `tr_record_calls::nest_root_shape_and_entry_guard_rules`.)
#[test]
fn link_enters_target_with_guards_rechecked() {
    let mut jobs = Vec::new();
    for t in ["always", "on2"] {
        for o in ["", "+exitstorm=3", "+verify"] {
            jobs.push(Job::new("nest_tr6d", t).opts(o));
        }
    }
    let clean = compare(jobs);
    assert!(
        clean.iter().any(|(_, o)| o.stats.nest_roots >= 1),
        "no nest root: {:?}",
        clean
            .iter()
            .map(|(_, o)| o.stats.line())
            .collect::<Vec<_>>()
    );
}

/// F2b: numeric `for` loops in a nest (`FORPREP` recorded for steps 1, -1, 3 and skipped loops; float loops and limits
/// are `Nyi`; side traces continue behind a loop that ends). Mutants `forprep_step_guard_skip`, `side_forloop_end_abort`.
#[test]
fn nest_for_loops() {
    for trace in ["always", "on2"] {
        let r = compare(vec![Job::new("nest_for_for", trace).opts("+verify")])
            .remove(0)
            .1;
        assert!(r.ok, "nest_for_for [{trace}]: {}", r.stderr);
        eprintln!("nest_for_for [{trace}] {}", r.stats.line());
        assert!(
            r.stats.nest_roots >= 1,
            "nest_for_for [{trace}]: {}",
            r.stats.line()
        );
        assert_eq!(
            r.stats.side_aborts.get("LeaveLoop").copied().unwrap_or(0),
            0,
            "{}",
            r.stats.line()
        );
        let r = run_all(vec![Job::new("nest_float_limit", trace)])
            .remove(0)
            .1;
        assert!(r.ok, "nest_float_limit [{trace}]: {}", r.stderr);
    }
}

/// P-F: loops with a run-time step (not a `LOADI` / `LOADK` literal right before the `FORPREP`) are traced with the
/// invariant step (`FORLOOP`) and a sign guard plus a variable divisor (`FORPREP` of a side trace); both signs, skipped
/// loops, huge steps and the error cases run identically. Mutants `forprep_var_sign_skip`, `forprep_step_always_var`.
#[test]
fn pf_variable_step_loops() {
    for trace in ["always", "on2"] {
        for opts in ["+verify", "", "+exitstorm=3"] {
            let r = compare(vec![Job::new("pf_var_step", trace).opts(opts)])
                .remove(0)
                .1;
            assert!(r.ok, "pf_var_step [{trace}] {opts}: {}", r.stderr);
            eprintln!("pf_var_step [{trace}] {opts}: {}", r.stats.line());
        }
        let r = compare(vec![Job::new("pf_var_step", trace).budget(17)])
            .remove(0)
            .1;
        assert!(r.ok);
    }
    // the sieve shape: the inner loop (step = the outer index) is one trace for all runs: no ineffective flush of it, no
    // blacklist. `sidemin=4` is the process default (P-F); the harness default is 0
    let mut j = Job::new("sieve_rounds", "on2");
    j.limits = "sidemin=4".into();
    let r = run_all(vec![j]).remove(0).1;
    assert!(r.ok, "{}", r.stderr);
    eprintln!("sieve_rounds [on2 sidemin=4] {}", r.stats.line());
    assert!(
        r.stats.blacklisted == 0 && r.stats.ineffective <= 1 && r.stats.installed <= 9,
        "sieve_rounds: {}",
        r.stats.line()
    );
    assert_eq!(
        r.stats.aborts.get("Identical").copied().unwrap_or(0),
        0,
        "{}",
        r.stats.line()
    );
    assert!(
        r.stats.side_aborts.get("SideShort").copied().unwrap_or(0) >= 1,
        "no SideShort abort: {}",
        r.stats.line()
    );
    // with no minimum the `FORPREP` of the inner loop is recorded in side traces: a run-time step must not make one side
    // trace per outer iteration (each guarded to its own step)
    let r = run_all(vec![Job::new("sieve_rounds", "on2")]).remove(0).1;
    assert!(r.ok, "{}", r.stderr);
    eprintln!("sieve_rounds [on2] {}", r.stats.line());
    assert!(
        r.stats.side_recorded <= 12,
        "sieve_rounds: {}",
        r.stats.line()
    );
}

/// P-L: an exit whose snapshot reads every scratch register still gets an inline stub (the exit counter lives in memory:
/// `cmp [ctr], hot-1; je divert; add [ctr], 1`), and its counter still reaches the hot threshold: the hot exit is handled
/// (a side recording is attempted at the vararg call) and the output is the interpreter's. Mutants
/// `exit_mem_counter_no_divert`, `exit_mem_counter_no_count`.
#[test]
fn pl_exit_counter_in_memory_without_a_free_register() {
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let r = compare(vec![Job::new("pl_varargs_exit", trace).opts(opts)])
                .remove(0)
                .1;
            assert!(r.ok, "pl_varargs_exit [{trace}] {opts}: {}", r.stderr);
            eprintln!("pl_varargs_exit [{trace}] {opts}: {}", r.stats.line());
            assert!(
                r.stats.side_recorded >= 1,
                "pl_varargs_exit [{trace}] {opts}: the hot exit was never handled: {}",
                r.stats.line()
            );
        }
    }
}

/// P-N: `%` / `//` by a constant power of two are an AND / an arithmetic shift; the result is the interpreter's for
/// every sign (trace modes `always`, `on2`, with `+verify`). Mutants `mod_pow2_mask_k`, `idiv_pow2_logical`.
#[test]
fn pn_pow2_divmod() {
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let r = compare(vec![Job::new("pn_pow2_divmod", trace).opts(opts)])
                .remove(0)
                .1;
            assert!(r.ok, "pn_pow2_divmod [{trace}] {opts}: {}", r.stderr);
        }
    }
}

/// P-V: `s:method` through the string metatable is a guarded constant resolution in traces; the metatable, its `__index`, the
/// table behind it and its methods are changed mid-loop (trace modes `always`, `on2`, with and without `+verify`). Mutant
/// `strmt_flush_skip`.
#[test]
fn pv_string_methods() {
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            // (K3b: `hdrlib=0` is the row that detects `strmt_flush_skip`: with the shipped `hdrlib=2` the stale constant is not observable on this program)
            for lim in ["", "hdrlib=0"] {
                let r = compare(vec![Job::new("pv_string_methods", trace)
                    .opts(opts)
                    .limits_s(lim)])
                .remove(0)
                .1;
                assert!(
                    r.ok,
                    "pv_string_methods [{trace}] {opts} {lim}: {}",
                    r.stderr
                );
            }
        }
    }
}

/// P-O: `pcall` / `xpcall` of a Lua closure is a trace construct whose body's guards exit before the call (trace modes `always`,
/// `on2`, with and without `+verify`). Mutants `pcall_impure_ignored`,
/// `pcall_shift_off`, `pcall_ccalls_skip`, `pcall_room_skip`, `ccalls_guard_off`.
#[test]
fn po_protected_calls() {
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let r = compare(vec![Job::new("po_protected_calls", trace).opts(opts)])
                .remove(0)
                .1;
            assert!(r.ok, "po_protected_calls [{trace}] {opts}: {}", r.stderr);
            assert!(
                r.stats.entered >= 1,
                "po_protected_calls [{trace}] {opts}: trace never entered: {}",
                r.stats.line()
            );
        }
    }
}

/// BUNDLE-G2: the C-stack guard of an inlined `pcall` is hoisted out of the loop body (checked once per entry): the loop is entered from
/// shallow and from deep `pcall` recursion, in every trace mode with `+verify`, exit storms and the stack-move stress. Mutant `g2_ccalls_no_hoist`
/// (an IR test, `tr_record_calls::g2_ccalls_guard_is_hoisted`, detects it) and `ccalls_guard_off` / `pcall_ccalls_skip`.
#[test]
fn g2_pcall_depth() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("g2_pcall_depth", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        jobs.push(Job::new("g2_pcall_depth", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" {
            assert!(
                o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// BUNDLE-H1: the limits a process (`rnlua`) runs with that matter for recursion and frame side traces (the struct defaults of the tests
/// have none of them).
const H1_PROC: &str = "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,sidemin=4,fnroot=1";

/// BUNDLE-H1: integer keys in the hash part on traces (`HREFV` integer form) in every trace mode with `+verify`, exit storms, small budgets,
/// the stack-move stress, the optimiser off, and with the feature off (`RUSTYNAIL_JIT_HASHINT=0`). Detector of the mutants `hrefi_mod_size`,
/// `hrefi_lsize_skip`, `hashint_nil_meta_skip`, `hashint_newindex_skip`.
#[test]
fn h1_hash_int_keys() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("", 17),
            ("-loop", 0),
            ("-fwd", 0),
        ] {
            let mut j = Job::new("h1_hash_int", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j.clone());
            // (the limits of a process: nil-valued nodes are stored into on traces)
            j.limits = H1_PROC.into();
            jobs.push(j);
        }
        jobs.push(Job::new("h1_hash_int", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
        jobs.push(Job::new("h1_hash_int", trace).env("RUSTYNAIL_JIT_HASHINT", "0"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() {
            assert!(
                o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// BUNDLE-H1: one trace serves every integer key of a big hash part: the exits of a loop over 200 hash keys stay a handful (a wrong main
/// position or a walk that stops early is a miss, an exit per key). Detector of `hrefi_mod_size`.
#[test]
fn h1_int_keys_do_not_exit_per_key() {
    let res = compare(vec![
        Job::new("h1_hash_int_many", "always").opts("+verify"),
        Job::new("h1_hash_int_many", "always"),
        Job::new("h1_hash_int_many", "on2"),
    ]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1,
            "{} {}: {}",
            j.prog,
            j.trace,
            o.stats.line()
        );
        // (one exit per leave of the inner loop: about 100; a miss per key would be 8000; 5.4 does not record hash-part integer keys)
        assert!(
            cfg!(rustynail_lua54) || o.stats.exits < 300,
            "{} {} {}: too many exits: {}",
            j.prog,
            j.trace,
            j.opts,
            o.stats.line()
        );
    }
}

/// BUNDLE-H1: the thresholds of the frame side traces of function-entry / continuation families: `fck` (an exit gets its side trace at
/// `fck` times the hot-exit count) and `framemin` (no side trace of fewer bytecodes that ends at a stitching exit). Detector of the mutants
/// `fck_ignored` and `framemin_ignored`: an unreachable threshold records fewer side traces than none.
#[test]
fn h1_frame_side_thresholds() {
    let job = |lim: &str| {
        let mut j = Job::new("h1_towers_piles", "always").opts("+verify");
        j.limits = format!("{H1_PROC},{lim}");
        j
    };
    let res = compare(vec![
        job("fck=1,framemin=0"),
        job("fck=1024,framemin=0"),
        job("fck=1,framemin=100000"),
    ]);
    let rec = |i: usize| res[i].1.stats.side_recorded;
    if cfg!(rustynail_lua54) {
        return; // (5.4 aborts `IntMiss` on the hash-part keys of the piles: the thresholds have nothing to act on)
    }
    assert!(
        rec(0) > rec(1),
        "an unreachable fck still records side traces ({} vs {}): {}",
        rec(0),
        rec(1),
        res[1].1.stats.line()
    );
    assert!(
        rec(0) > rec(2),
        "an unreachable framemin still records side traces ({} vs {}): {}",
        rec(0),
        rec(2),
        res[2].1.stats.line()
    );
}

/// BUNDLE-H1: towers with a key-0 pile: the exits inside inlined frames of function-entry / continuation traces get side traces with virtual
/// frames (`fcfn`), in every trace mode with `+verify`, exit storms, small budgets, low `fck` / `framemin` (every exit gets one). Detector of the mutants `fcfn_cells_off` / `fcfn_family_excluded` (no side trace recorded in those families).
#[test]
fn h1_towers_frame_sides() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for (opts, lim) in [
            ("", ""),
            ("+verify", ""),
            ("+exitstorm=3 +verify", ""),
            ("+verify", "fck=1,framemin=0"),
            ("+verify", "fck=2,framemin=3"),
            ("+verify", "fck=1,framemin=0,unroll=4,maxdepth=6"),
        ] {
            let mut j = Job::new("h1_towers_piles", trace).opts(opts);
            j.limits = format!("{H1_PROC},{lim}");
            jobs.push(j);
        }
        for b in [1, 17] {
            let mut j = Job::new("h1_towers_piles", trace).budget(b);
            j.limits = format!("{H1_PROC},fck=1,framemin=0");
            jobs.push(j);
        }
        let mut j = Job::new("h1_towers_piles", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = format!("{H1_PROC},fck=1,framemin=0");
        jobs.push(j);
    }
    let res = compare(jobs);
    let rec = |lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.trace == "always" && j.opts == "+verify" && j.limits == format!("{H1_PROC},{lim}")
            })
            .map(|(_, o)| o.stats.side_recorded)
            .unwrap()
    };
    assert!(
        rec("fck=1,framemin=0") >= 30,
        "h1_towers_piles: the frame cells of function-entry / recursion traces record side traces ({})",
        rec("fck=1,framemin=0")
    );
}

/// H3-A: an exit that is blacklisted (it will never get a side trace) is not reported to the exit handler any more: the frame-materialising
/// stub of an unattached frameful exit diverted every `hot_exit`-th exit to the handler, which had nothing to do for a blacklisted one (queens:
/// 94% of the handler calls). The counter of a blacklisted exit carries `EXIT_BLACK_BIT`, which the stub's periodic test masks in. Detector of
/// the mutant `black_exit_report` (the bit is never set: every blacklisted exit is still reported).
#[test]
fn h3_black_exits_not_reported() {
    let mut jobs = Vec::new();
    // (BUNDLE-VA: with the shipped `sidenest=1` the frameful exits of these programs link instead of using the materialising stub, so no blacklisted
    // exit reaches the handler even with the mutant; `sidenest=0` restores the stub path: 2287 reports of `be_queens` with the mutant)
    for (name, trace) in [("be_queens", "on"), ("h1_towers_piles", "on")] {
        for extra in ["", ",sidenest=0"] {
            let mut j = Job::new(name, trace);
            j.limits = format!("{H1_PROC}{extra}");
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    let mut total = 0;
    for (j, o) in &res {
        assert!(o.ok, "{} {}: {}", j.prog, j.limits, o.stderr);
        total += o.stats.exits;
        assert_eq!(
            o.stats.black_handled,
            0,
            "{} {}: blacklisted exits reached the handler: {}",
            j.prog,
            j.limits,
            o.stats.line()
        );
    }
    assert!(total > 10000, "the programs exit through traces ({total})");
}

/// P-X: a trace exit lands on a tier-1 label whose register types were derived from the straight-line code before it
/// (`lower`: `land_checks`): the float case of `%` / `//` that tier 1 exits on is handled by the trace, so the label verifies
/// the tags (trace modes `always`, `on2`, with and without `+verify`). Mutant `land_type_check_skip`.
#[test]
fn px_typed_landing_labels() {
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let r = compare(vec![Job::new("px_typed_landing", trace).opts(opts)])
                .remove(0)
                .1;
            assert!(r.ok, "px_typed_landing [{trace}] {opts}: {}", r.stderr);
            assert!(
                r.stats.side_recorded >= 1,
                "px_typed_landing [{trace}] {opts}: {}",
                r.stats.line()
            );
        }
    }
}

/// P-M4: the P-X landing rule on the native-recursion paths (`recnat=1`): floats in an argument / a result behind the int-only
/// partial-leaf templates `%`, `//`, `&`, `|`, with traces `always`/`on2`, `+verify`, `+emurun`, a deeper unroll, small budgets.
/// Mutant `land_type_check_skip` must be caught by it as well.
#[test]
fn pm4_recursion_typed_landing() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for opts in ["", "+verify", "+emurun", "+exitstorm=3 +verify"] {
            for lim in ["recnat=1", "recnat=1,unroll=4,maxdepth=6"] {
                let mut j = Job::new("px_rec_landing", trace).opts(opts);
                j.limits = lim.into();
                jobs.push(j);
            }
        }
        for b in [1, 17] {
            let mut j = Job::new("px_rec_landing", trace).budget(b);
            j.limits = "recnat=1".into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    for (j, o) in res.iter().filter(|(j, _)| !j.opts.contains("emurun")) {
        // (under `+emurun` the emulator refuses the native self-call / return tails: `Compile(emu-unsupported)`, nothing runs)
        assert!(
            o.stats.entered >= 1 || o.stats.exits > 0,
            "px_rec_landing [{}] {}: no trace ran: {}",
            j.trace,
            j.opts,
            o.stats.line()
        );
    }
    // (the side recordings at the float iterations are what puts the exits mid-block)
    assert!(
        res.iter()
            .any(|(j, o)| j.trace == "always" && j.opts.is_empty() && o.stats.side_recorded >= 1),
        "px_rec_landing: no side trace recorded"
    );
}

/// P-M4: an inner entry (native return / push) books no entry, so a failing entry check there must not be taken for the failure
/// of a booked entry (stale first-iteration word: `entry_ok` wrapped to 2^32-1 and `entered` read ~4e9 per head). The sum of
/// `entry_ok` of a short program stays far below that. Mutant `rec_inner_first_stale`.
#[test]
fn pm4_recursion_entry_counters() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let mut j = Job::new("pm4_rec_counters", trace).opts(opts);
            j.limits = "recnat=1".into();
            jobs.push(j);
        }
    }
    for (j, o) in &compare(jobs) {
        assert!(
            o.stats.entered < 1 << 24,
            "pm4_rec_counters [{}] {}: entry counters wrapped: {}",
            j.trace,
            j.opts,
            o.stats.line()
        );
    }
}

/// F2a: programs whose hot exits must produce a side trace that runs (7.8, 7.9).
const MUST_SIDE: &[&str] = &[
    "side_branch",
    "side_nested",
    "side_entry0",
    "side_entry0_mixed",
    "side_midblock",
    "side_calls",
    "side_to_other_head",
    "side_poly_types",
    "side_many",
    "type_entry",
];

/// F2a: every program of `MUST_SIDE` installs a side trace, and the side trace RUNS: with `+verify` a side trace counts
/// its executions like a loop counts iterations, so the counter exceeds the one of a run without side traces
/// (`maxside=0`, the prototype's policy). `side_untraced_head` reaches a head without a trace (7.8: abort).
#[test]
fn side_traces_install_and_run() {
    for trace in ["always", "on2"] {
        for p in MUST_SIDE {
            let with = run_all(vec![Job::new(p, trace).opts("+verify")])
                .remove(0)
                .1;
            assert!(with.ok, "{p} [{trace}]: {}", with.stderr);
            let mut j = Job::new(p, trace).opts("+verify");
            j.limits = "maxside=0".into();
            let without = run_all(vec![j]).remove(0).1;
            assert!(without.ok, "{p} [{trace}] maxside=0: {}", without.stderr);
            eprintln!(
                "{p:<20} [{trace}] iterations {} (maxside=0: {}) {}",
                with.iterations,
                without.iterations,
                with.stats.line()
            );
            assert_eq!(
                without.stats.side_recorded, 0,
                "{p}: maxside=0 records nothing"
            );
            assert!(
                with.stats.side_installed >= 1,
                "{p} [{trace}]: no side trace: {}",
                with.stats.line()
            );
            if *p == "side_entry0_mixed" {
                // the prelude hands the entries it cannot serve (float accumulator) to the root trace: the root is
                // entered as often as without it (mutant `prelude_no_chain`: those runs stay in tier 1)
                assert!(
                    with.stats.entered + 8 >= without.stats.entered,
                    "{p} [{trace}]: root entered {} times, {} without the prelude",
                    with.stats.entered,
                    without.stats.entered
                );
            }
            assert!(
                with.iterations > without.iterations,
                "{p} [{trace}]: the side trace never ran ({} vs {} iterations): {}",
                with.iterations,
                without.iterations,
                with.stats.line()
            );
        }
    }
    // side traces shorter than `sidemin` are refused (the default is 8; the harness runs the twins with 0)
    for trace in ["always", "on2"] {
        let mut j = Job::new("side_branch", trace);
        j.limits = "sidemin=1000,retf=2".into(); // (a side recording that popped into its caller ends with a stitching exit that `sidemin` does not measure)
        let r = run_all(vec![j]).remove(0).1;
        assert!(r.ok, "side_branch sidemin: {}", r.stderr);
        assert_eq!(r.stats.side_installed, 0, "{}", r.stats.line());
        assert!(
            r.stats.side_aborts.get("SideShort").copied().unwrap_or(0) >= 1,
            "side_branch [{trace}] sidemin=1000: {}",
            r.stats.line()
        );
    }
    // the generic exit path (`+emuverify` sends every exit through the handler) follows the exit cells too, and
    // `maxside` bounds the side traces of a family
    for trace in ["always", "on2"] {
        let with = run_all(vec![
            Job::new("side_branch", trace).opts("+verify +emuverify")
        ])
        .remove(0)
        .1;
        let mut j = Job::new("side_branch", trace).opts("+verify +emuverify");
        j.limits = "maxside=0".into();
        let without = run_all(vec![j]).remove(0).1;
        assert!(
            with.ok && without.ok,
            "{} / {}",
            with.stderr,
            without.stderr
        );
        assert!(
            with.iterations > without.iterations,
            "side_branch [{trace} +emuverify]: the generic path never entered the side trace ({} vs {})",
            with.iterations,
            without.iterations
        );
        let mut j = Job::new("side_many", trace);
        j.limits = "maxside=1".into();
        let one = run_all(vec![j]).remove(0).1;
        assert!(one.ok, "side_many maxside=1: {}", one.stderr);
        assert!(
            one.stats.side_installed <= 1,
            "side_many [{trace}] maxside=1: {}",
            one.stats.line()
        );
    }
    for trace in ["always", "on2"] {
        // (`recnat=0,recany=0`: with the shipped function-entry heads `f` is a trace root of its own and the `while` head is reached through it; the
        // abort this test is about is the one of a side recording that meets a head without any trace)
        let mut j = Job::new("side_untraced_head", trace);
        j.limits = "recnat=0,recany=0,libcall=7".into();
        let r = run_all(vec![j]).remove(0).1;
        assert!(r.ok, "side_untraced_head: {}", r.stderr);
        assert!(
            r.stats
                .side_aborts
                .get("SideUntraced")
                .copied()
                .unwrap_or(0)
                >= 1,
            "side_untraced_head [{trace}]: {}",
            r.stats.line()
        );
    }
}

/// F2a (7.9, TR6d): the entry-0 side trace of a loop whose root trace fails its entry check at (nearly) every entry
/// becomes the prelude of the head: the loop is entered through it and the failing root entry (a second exit per run)
/// is gone. `side_entry0_mixed` keeps both kinds of entries going. Mutants `prelude_never`, `prelude_no_chain`.
#[test]
fn entry0_prelude() {
    for trace in ["always", "on2"] {
        let e = run_all(vec![Job::new("type_entry", trace)]).remove(0).1;
        assert!(e.ok, "type_entry: {}", e.stderr);
        eprintln!("type_entry [{trace}] {}", e.stats.line());
        assert_eq!(
            e.stats.side_preludes,
            1,
            "type_entry [{trace}]: {}",
            e.stats.line()
        );
        // one exit per run (the loop end), not two (the failing entry as well, which the prelude takes over)
        let (exits, entered) = (e.stats.exits, e.stats.entered);
        assert!(
            entered >= 300 && exits * 10 <= entered * 13,
            "type_entry [{trace}]: {exits} exits for {entered} entries: {}",
            e.stats.line()
        );
        let m = run_all(vec![Job::new("side_entry0_mixed", trace).opts("+verify")])
            .remove(0)
            .1;
        assert!(m.ok, "side_entry0_mixed: {}", m.stderr);
        assert_eq!(m.stats.side_preludes, 1, "{}", m.stats.line());
    }
}

/// Q2: a trace that leaves through a pre-roll guard at (nearly) every entry is flushed by the ineffective-trace rule
/// (`cache::pre_exit_check`) and the head ends Black after the re-record limit; a healthy trace with the same shape is
/// untouched. `ineff_marking` is the sieve pattern: the recorded step guard fails at every iteration entry after the
/// first prime.
// needs the mutant `forloop_step_always_lit` (feature `tr_mutants`): without it the recorder no longer guards a run-time
// step as a constant and the pattern produces no ineffective trace
#[cfg(feature = "tr_mutants")]
#[test]
fn ineffective_trace_rule() {
    for trace in ["always", "on2"] {
        // P-F: a run-time loop step is no longer guarded as a constant, so the sieve pattern does not produce an ineffective
        // trace by itself any more; the mutant `forloop_step_always_lit` brings the old recording back (appended to the mutant
        // the runner may have activated for this test)
        let mut muts = std::env::var("RUSTYNAIL_JIT_MUT").unwrap_or_default();
        if !muts.is_empty() {
            muts.push(',');
        }
        muts.push_str("forloop_step_always_lit");
        let r = run_all(vec![
            Job::new("ineff_marking", trace).env("RUSTYNAIL_JIT_MUT", &muts)
        ])
        .remove(0)
        .1;
        assert!(r.ok, "ineff_marking: {}", r.stderr);
        eprintln!("ineff_marking [{trace}] {}", r.stats.line());
        let s = &r.stats;
        assert!(
            s.ineffective >= 1,
            "ineff_marking [{trace}]: the rule did not fire: {}",
            s.line()
        );
        assert!(
            s.blacklisted >= 1,
            "ineff_marking [{trace}]: the head must end Black: {}",
            s.line()
        );
        // at most a few thousand wasted entries per flushed trace (the rule evaluates every 1024 exits), nowhere
        // near one entry per iteration (hundreds of thousands)
        assert!(
            s.exits <= 64 * 1024,
            "ineff_marking [{trace}]: too many exits: {}",
            s.line()
        );
        // the healthy twin (the same loop, constant step) is never flushed by the rule
        let h = run_all(vec![Job::new("ineff_marking_stable", trace)])
            .remove(0)
            .1;
        assert!(h.ok, "ineff_marking_stable: {}", h.stderr);
        assert_eq!(h.stats.ineffective, 0, "stable twin: {}", h.stats.line());
    }
}

/// Q3: a head whose flushed trace (Rerec / Ineffective) is compiled again to the identical shape is blacklisted
/// instead of installing the same trace once more, and the second Ineffective flush of a head blacklists at once.
/// `sieve_rounds` installed 12 traces (4 identical generations per loop) before Q3.
#[test]
fn identical_regeneration_is_not_installed() {
    for trace in ["always", "on2"] {
        // (`maxside=0`: the prototype's policy. With side traces the same program ping-pongs through different
        // recordings and the rule is exercised less often, see `side_*` for the full-build behaviour)
        let mut j = Job::new("sieve_rounds", trace);
        j.limits = "maxside=0".into();
        let r = run_all(vec![j]).remove(0).1;
        assert!(r.ok, "sieve_rounds: {}", r.stderr);
        eprintln!("sieve_rounds [{trace}] {}", r.stats.line());
        let s = &r.stats;
        assert!(
            s.installed <= 9 // (F3: the fill loop is traced too)
                && s.flushes <= 9
                && s.aborts.get("Identical").copied().unwrap_or(0) >= 1,
            "sieve_rounds [{trace}]: too many installs: {}",
            s.line()
        );
    }
}

/// The traced runs of the bench programs' shapes keep working under `+verify` with iteration counting: the trace ran.
#[test]
fn iterations_are_counted_under_verify() {
    let r = run_all(vec![Job::new("int_sum", "always").opts("+verify")])
        .remove(0)
        .1;
    assert!(r.ok);
    assert!(r.iterations > 1000, "iterations {}", r.iterations);
}

/// 9.5: `fetches(off) == fetches(run) + Stats::insns(run)` for every program, every budget, trace modes and options.
#[cfg(feature = "t15_oracle")]
#[test]
fn instruction_count_identity() {
    let mut jobs = Vec::new();
    for p in selected(false) {
        for b in [None, Some(3), Some(17), Some(200)] {
            for (t, o) in [
                ("always", ""),
                ("on2", ""),
                ("always", "-fold -cse +exitstorm=7"),
                ("always", "+emurun"),
                ("on", ""),
            ] {
                let mut j = Job::new(p.name, t).opts(o);
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
            // F2b: linking exits and stitching on: the count model holds across links
            for t in ["always", "on2"] {
                let mut j = Job::new(p.name, t);
                j.budget = b;
                j.ident = true;
                j.limits = "nest=2,stitchmin=4".into();
                jobs.push(j);
            }
        }
    }
    let n = jobs.len();
    let res = run_all(jobs);
    let mut bad = Vec::new();
    for (j, o) in &res {
        match ident_check(o) {
            Ok(l) if verbose() => eprintln!("{:<70} {l}", j.tag()),
            Ok(_) => {}
            Err(e) => bad.push(format!("{}: {e}", j.tag())),
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {n} identity failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Q5: the interpreter takes over from a trace exit that starts a side recording at a tier-1 label: tier 1 would give the
/// prepaid rest of the block back (`interp_rem`, `exit.rs`), so `fetches + insns` must still equal the interpreter's count.
/// A small, fixed family (the jobs start side recordings: asserted) that does not depend on the 3976-job
/// `instruction_count_identity` run, which the mutant runner could not rely on under load (mutant `side_interp_no_rem`).
#[cfg(feature = "t15_oracle")]
#[test]
fn side_recording_start_keeps_the_count_identity() {
    let mut jobs = Vec::new();
    for p in ["int_sum", "side_branch", "side_midblock", "nest_for_for"] {
        for b in [None, Some(3), Some(17), Some(200)] {
            let mut j = Job::new(p, "always");
            j.budget = b;
            j.ident = true;
            jobs.push(j);
        }
    }
    let n = jobs.len();
    let res = run_all(jobs);
    let (mut bad, mut started) = (Vec::new(), 0);
    for (j, o) in &res {
        if let Err(e) = ident_check(o) {
            bad.push(format!("{}: {e}", j.tag()));
        }
        started += o.stats.side_recorded;
    }
    assert!(
        started > 0,
        "no job started a side recording: the test is vacuous"
    );
    assert!(
        bad.is_empty(),
        "{} of {n} identity failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn programs_are_deterministic() {
    // the interpreter baseline of every program is stable (twin comparisons are meaningful)
    let progs = selected(false);
    let a = run_all(progs.iter().map(|p| Job::new(p.name, "off")).collect());
    let b = run_all(progs.iter().map(|p| Job::new(p.name, "off")).collect());
    for ((j, x), (_, y)) in a.iter().zip(&b) {
        assert!(x.ok && y.ok, "{}", j.prog);
        assert_eq!(x.result, y.result, "{} is not deterministic", j.prog);
        assert!(
            x.result.contains("STATUS true"),
            "{} failed in the interpreter: {}",
            j.prog,
            x.result
        );
    }
}

/// `TR_CORE_DUMP=<dir>`: writes every program as `<dir>/<name>.lua` (for `rnlua` runs of the same corpus).
#[test]
fn dump_programs() {
    if let Ok(d) = std::env::var("TR_CORE_DUMP") {
        std::fs::create_dir_all(&d).expect("dir");
        for p in PROGS.iter().chain(SINK_PROGS).chain(NR_PROGS) {
            std::fs::write(format!("{d}/{}.lua", p.name), p.src).expect("write");
        }
    }
}

// ---- F3: class-A helpers (TR1): TNEW, SETLIST, NEWREF, TLEN ---------------------------------------------------------------

/// The allocation programs whose loops record and run with class-A helpers.
const CLASS_A: &[&str] = &[
    "alloc_ctor_array",
    "alloc_ctor_hash",
    "alloc_len",
    "alloc_newref_present",
    "alloc_newindex",
    "alloc_escape",
    "alloc_churn",
    "alloc_churn_gen",
    "alloc_churn_inc",
    "alloc_large_ctor",
    "alloc_interleaved",
    "alloc_append",
    "alloc_append_meta",
    "alloc_pool",
];

/// S0: the allocation-sinking programs (`tr_core/sink_progs.rs`).
const SINK: &[&str] = &[
    "sink_basic",
    "sink_exit_mid",
    "sink_alias_frames",
    "j1_sinkf",
    "j1_sinkf_mem_error",
    "sinkf_side",
    "sinkf_side_mem_error",
    "sinkroot_pre",
    "sink_live_local",
    "sink_hooks_getlocal",
    "sink_tail_multret",
    "sink_count",
    "sink_count_stopped",
    "sink_mem_error",
    "sink_pacing",
];

/// The allocation programs install and enter traces (the helpers run natively), in `always` and `on2`, with the lockstep
/// (`+verify`) and with every guard failing now and then (`+exitstorm`): the same results as the interpreter.
#[test]
fn class_a_helpers_run_on_traces() {
    let mut jobs = Vec::new();
    for p in CLASS_A {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// Under GC stress the interpreter steps the collector at every allocation, so the helpers always report "a step is due"
/// and the traces exit before each allocating instruction; the results stay those of the interpreter, and with the
/// collector verifier on the smallest programs.
#[test]
fn class_a_helpers_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in CLASS_A {
        if p.starts_with("alloc_churn") {
            continue; // not small
        }
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p, "on2")
                    .opts("+verify")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
    }
    // the allocation-heavy programs in both collector modes without stress
    for p in ["alloc_churn", "alloc_churn_gen", "alloc_churn_inc"] {
        jobs.push(Job::new(p, "always"));
        jobs.push(Job::new(p, "on2"));
    }
    compare(jobs);
}

/// TR32 (acceptance of F3): a GC step during the recording frees an inlined callee whose closure was overwritten. The
/// edge registered when the call was inlined makes `free_code` abort the recording (`DepFreed`, no penalty): no trace
/// keeps a callee-identity guard on the address of a freed prototype. Mutant `dep_edge_at_install` registers the edge at
/// install, after the free: the recording completes with a stale guard (and the install reads the freed prototype).
#[test]
fn proto_churn_aborts_the_recording_with_dep_freed() {
    for trace in ["always", "on2"] {
        let j = Job::new("alloc_proto_churn", trace)
            .opts("+verify")
            .env("RUSTYNAIL_GC_STRESS", "1");
        let r = compare(vec![j]).remove(0).1;
        assert!(
            r.stats.aborts.get("DepFreed").copied().unwrap_or(0) >= 1,
            "alloc_proto_churn [{trace}]: no recording was aborted by a freed dependency: {}",
            r.stats.line()
        );
    }
}

/// TR32: a root freed before its callee: the root's trace is flushed when its prototype is freed, the later free of the
/// callee finds a stale `TrId` and flushes nothing (no second flush, no flush of a reused slot).
#[test]
fn root_freed_before_its_callee() {
    for trace in ["always", "on2"] {
        for (opts, stress) in [
            ("+verify", None),
            ("+verify +tracemove", None),
            ("", Some("2")),
        ] {
            let mut j = Job::new("alloc_root_before_callee", trace).opts(opts);
            if let Some(s) = stress {
                j = j.env("RUSTYNAIL_GC_STRESS", s);
            }
            let r = compare(vec![j]).remove(0).1;
            assert!(
                r.stats.flushes <= r.stats.installed + r.stats.side_installed,
                "alloc_root_before_callee [{trace} {opts}]: {} flushes for {} installed traces: {}",
                r.stats.flushes,
                r.stats.installed + r.stats.side_installed,
                r.stats.line()
            );
        }
    }
}

// ---- F3b: generic `for` (ipairs / next), CONCAT, STRTO ------------------------------------------------------------------

/// The F3b programs.
const F3B: &[&str] = &[
    "tfor_ipairs",
    "tfor_closure",
    "tfor_pairs",
    "tfor_errors",
    "tfor_te1",
    "tfor_te2",
    "tfor_te3",
    "tfor_te4",
    "tfor_te5",
    "tfor_te6",
    "str_concat",
    "strto_for",
    "str_temporaries",
    "tfor_iter_swap",
];

/// The F3b programs install and enter traces, in `always` and `on2`, with the lockstep (`+verify`), with every guard failing
/// now and then (`+exitstorm`) and with small budgets: the same results as the interpreter.
#[test]
fn f3b_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in F3B {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always"
            && j.opts == "+verify"
            && !["tfor_te2", "tfor_te4"].contains(&j.prog.as_str())
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// GC stress 1 / 2 (and the collector verifier on the smallest programs) over the F3b programs, in both collector modes.
#[test]
fn f3b_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in F3B {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p, "on2")
                    .opts("+verify")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
        // both collector modes, with and without stress
        for mode in ["gen", "inc"] {
            for stress in [None, Some("2")] {
                let mut j = Job::new(p, "always")
                    .opts("+verify")
                    .env("TR_CORE_GCMODE", mode);
                if let Some(st) = stress {
                    j = j.env("RUSTYNAIL_GC_STRESS", st);
                }
                jobs.push(j);
            }
        }
    }
    compare(jobs);
}

// ---- F4: recursion unroll, TAILCALL, multiple results with an explicit `top` ---------------------------------------------

/// The F4 programs.
const F4: &[&str] = &[
    "rec_fib",
    "rec_ack",
    "rec_depth",
    "rec_depth_locals",
    "rec_mutual",
    "rec_tree",
    "rec_error",
    "rec_error_deep",
    "rec_coroutine",
    "rec_coroutine_tail",
    "tail_loop",
    "tail_args",
    "tail_mutual",
    "tail_flag",
    "tail_locals",
    "swap_divmod",
    "multret_calls",
    "multret_return",
    "multret_intrinsic",
    "multret_cfunc",
    "multret_setlist",
    "vararg_select",
    "multret_vararg_root",
];

/// The F4 programs in every mode with the lockstep (`+verify`: a recorder model of `top`, of a replaced frame or of a
/// recursive frame that disagrees with the interpreter panics), with every guard failing now and then (`+exitstorm`: every
/// exit of every snapshot, with frames, replaced frames and `top`, is taken and compared) and with small budgets.
#[test]
fn f4_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in F4 {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=1 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
    }
    compare(jobs);
}

/// P-G: `TrLimits::warm` (time stamp counter ticks between the first loop activity and the first recording): parses, defaults to 0 for the plain
/// configuration (the process default, 1200000, comes from the environment), and a gate longer than the program
/// runs means nothing is ever recorded, while a gate of 2 only delays the first recording.
#[test]
fn pg_warm_gate_delays_the_first_recording() {
    assert_eq!(TrLimits::default().warm, 0);
    assert_eq!(TrLimits::parse("warm=7").0.warm, 7);
    assert!(TrLimits::parse("warm=-1").1.len() == 1);
    assert_eq!(rn_jit::tr::cache::DEFAULT_WARM, 1_200_000);
    let mut jobs = Vec::new();
    for w in ["warm=4000000000", "warm=2", "warm=0"] {
        let mut j = Job::new("int_sum", "on2").opts("+verify");
        j.limits = format!("{w},fnroot=0");
        jobs.push(j);
    }
    let res = compare(jobs);
    assert_eq!(
        res[0].1.stats.recorded,
        0,
        "a gate beyond the program's firings: {}",
        res[0].1.stats.line()
    );
    assert_eq!(res[0].1.stats.installed, 0);
    assert!(
        res[1].1.stats.installed >= 1,
        "warm=2 only delays: {}",
        res[1].1.stats.line()
    );
    assert!(res[2].1.stats.installed >= res[1].1.stats.installed.min(1));
}

/// The recursion limit and the stitching exit beyond it: unroll 0 (the prototype's abort), 1, 2, 3 and stitching at four
/// bytecodes (so that the exit beyond the limit is taken), in every mode.
#[test]
fn f4_unroll_limits_and_the_exit_beyond() {
    let mut jobs = Vec::new();
    for p in F4 {
        for l in [
            "unroll=0",
            "unroll=1,stitchmin=4",
            "unroll=2,stitchmin=4",
            "unroll=3,stitchmin=4",
            "unroll=2,stitchmin=4,maxside=0",
        ] {
            for trace in ["always", "on2"] {
                // (`fnroot=0`: these are the F4 loop recordings, with `table.pack` as the stitching C call of `multret_cfunc`; P-E2 function-entry roots are covered by the `pe2_*` tests)
                let mut j = Job::new(p, trace).opts("+verify");
                j.limits = format!("{l},fnroot=0");
                jobs.push(j);
                let mut j = Job::new(p, trace).opts("+exitstorm=3");
                j.limits = format!("{l},fnroot=0");
                jobs.push(j);
            }
        }
    }
    compare(jobs);
}

/// The F4 programs install traces that run (and, for the recursion programs, leave them again through the exit beyond the
/// unroll limit), and the counters say what the recorder did.
#[test]
fn f4_programs_install_and_enter_traces() {
    let must = [
        "rec_fib",
        "rec_ack",
        "rec_depth",
        "rec_mutual",
        "rec_tree",
        "tail_loop",
        "tail_args",
        "tail_mutual",
        "tail_flag",
        "swap_divmod",
        "multret_calls",
        "multret_return",
        "multret_intrinsic",
    ];
    let mut jobs = Vec::new();
    for p in must {
        let mut j = Job::new(p, "always").opts("+verify");
        j.limits = "stitchmin=4".into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
            "{}: no trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// GC stress 1 / 2 over the F4 programs (the exits materialise frames above stale registers; the collector scans them).
#[test]
fn f4_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in F4 {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            let mut j = Job::new(p, "on2")
                .opts("+verify")
                .env("RUSTYNAIL_GC_STRESS", stress);
            j.limits = "stitchmin=4".into();
            jobs.push(j);
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
        for mode in ["gen", "inc"] {
            jobs.push(
                Job::new(p, "always")
                    .opts("+verify")
                    .env("TR_CORE_GCMODE", mode),
            );
        }
    }
    compare(jobs);
}

// ---- P-E: inlined vararg callees, `select`, loops of callees unrolled --------------------------------------------------

/// The P-E programs.
const PE: &[&str] = &[
    "vararg_fwd",
    "vararg_loop",
    "vararg_types",
    "vararg_nested",
    "vararg_misc",
    "vararg_select",
    "multret_vararg_root",
];

/// The P-E programs in every mode with the lockstep (`+verify`: the recorder's model of the vararg frame layout, of `top` after
/// `VARARG C=0` and of the `select` results against the interpreter), with every guard failing now and then (`+exitstorm`:
/// every exit of every snapshot, including frames with `nextraargs`, is taken and compared) and with small budgets.
#[test]
fn pe_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in PE {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=1 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
        for l in ["loopunroll=0", "loopunroll=3", "maxdepth=2"] {
            let mut j = Job::new(p, "always").opts("+verify");
            j.limits = l.into();
            jobs.push(j);
        }
    }
    compare(jobs);
}

// ---- P-E2: function-entry roots, `table.pack` -------------------------------------------------------------------------------

/// The P-E2 programs.
const PE2: &[&str] = &[
    "pack_basic",
    "fnroot_fib",
    "fnroot_mutual",
    "fnroot_error",
    "fnroot_coro",
    "fnroot_hook",
    "fnroot_types",
];

/// The P-E2 programs in every mode with the lockstep, exit storms (every exit of every snapshot is taken and compared), small
/// budgets and the unroll / depth limits that make the function roots record deeper trees.
#[test]
fn pe2_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in PE2 {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=1 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
        for l in [
            "fnroot=0",
            "fnroot=1",
            "fnroot=1,unroll=4,maxdepth=6,fnmin=4",
            "fnroot=1,unroll=8,maxdepth=8,fnmin=8,maxside=0",
            "fnroot=1,fnmin=0",
        ] {
            let mut j = Job::new(p, "always").opts("+verify");
            j.limits = l.into();
            jobs.push(j);
        }
    }
    // (function-entry roots are off by default: the jobs without limits turn them on)
    for j in jobs.iter_mut().filter(|j| j.limits.is_empty()) {
        j.limits = "fnroot=1".into();
    }
    compare(jobs);
}

/// A function root whose entries leave through a guard of the recorded path (nothing after it runs) is flushed by the
/// ineffective-trace rule (`P >= 20%` of the entries for a function root). Mutants `fn_exits_not_pre`, `fn_ineff_pct_ignored`.
fn ineffective_fn_root(prog: &str) {
    let mut j = Job::new(prog, "always");
    // (no side traces: an exit with a side trace attached bypasses the handler that evaluates the rule)
    j.limits = "fnroot=1,fnmin=4,maxside=0".into();
    let res = compare(vec![j]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1,
            "{}: no function root was installed: {}",
            j.prog,
            o.stats.line()
        );
        assert!(
            o.stats.ineffective >= 1,
            "{}: the ineffective function root was not flushed: {}",
            j.prog,
            o.stats.line()
        );
    }
}

/// The final exit of a function root (before the root frame's `RETURN`) is taken by every entry; it gets no side trace
/// (an empty one would only add an entry, an exit and another hot exit). Mutant `fn_side_at_return`.
#[test]
fn fn_root_final_exit_gets_no_side_trace() {
    let mut j = Job::new("fnroot_mixed", "always");
    j.limits = "fnroot=1,fnmin=4,maxside=8".into();
    let res = compare(vec![j]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1,
            "{}: no function root was installed: {}",
            j.prog,
            o.stats.line()
        );
        assert!(
            o.stats.side_installed < 8,
            "{}: a chain of empty side traces: {}",
            j.prog,
            o.stats.line()
        );
    }
}

/// A function whose recordings all end short (a leaf call, below `fnmin` bytecodes) is blacklisted after `MAX_FN_SHORT`
/// attempts, not after the 16 of a short loop. Mutant `fn_short_budget_ignored`.
#[test]
fn fn_root_short_recordings_blacklist_the_head() {
    // `fnpromo=0` is the rule itself (four short recordings blacklist the head); the shipped `fnpromo` gives the head one more recording
    // (a promotion round) before it is blacklisted for good
    let mut j = Job::new("fnroot_leafy", "always");
    // (`centry=0`: the program's `leafy` is called from `gsub`, a C-entered function root that is exempt from `fnmin`)
    j.limits = "fnroot=1,fnmin=64,fnpromo=0,centry=0".into();
    let mut j2 = Job::new("fnroot_leafy", "always");
    j2.limits = "fnroot=1,fnmin=64,centry=0".into();
    let res = compare(vec![j, j2]);
    for (j, o) in &res {
        let short = o.stats.aborts.get("FnShort").copied().unwrap_or(0);
        let want = if j.limits.contains("fnpromo=0") { 4 } else { 5 };
        assert_eq!(
            short,
            want,
            "{} [{}]: the head is blacklisted after {want} short recordings: {}",
            j.prog,
            j.limits,
            o.stats.line()
        );
    }
}

#[test]
fn fn_root_with_a_dead_path_is_flushed_ineffective() {
    ineffective_fn_root("fnroot_dead");
}

#[test]
fn fn_root_with_a_mixed_path_is_flushed_ineffective() {
    ineffective_fn_root("fnroot_mixed");
}

/// GC stress 1 / 2 over the P-E2 programs.
#[test]
fn pe2_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in PE2 {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p, "on2")
                    .opts("+verify")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
        for mode in ["gen", "inc"] {
            jobs.push(
                Job::new(p, "always")
                    .opts("+verify")
                    .env("TR_CORE_GCMODE", mode),
            );
        }
    }
    // (function-entry roots are off by default: the jobs without limits turn them on)
    for j in jobs.iter_mut().filter(|j| j.limits.is_empty()) {
        j.limits = "fnroot=1".into();
    }
    compare(jobs);
}

/// `table.pack` is recorded (no stitching exit at it) and a function-entry root is installed and entered for the recursion
/// programs; the counters say what the recorder did.
#[test]
fn pe2_programs_install_and_enter_traces() {
    let mut jobs = Vec::new();
    for p in ["pack_basic", "fnroot_fib", "fnroot_mutual"] {
        let mut j = Job::new(p, "always").opts("+verify");
        j.limits = "fnroot=1,unroll=4,maxdepth=6,fnmin=4".into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
            "{}: no trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// R2: native recursion for methods / globals / loops and `RETURN0` (`recany=1`, on top of `recnat=1`): the program `r2_rec`
/// (permute, hanoi, call sites with 0 / 1 / 2 / all wanted results, errors, a coroutine, GC and a hook inside the recursion) in the trace
/// modes with lockstep, exit storms, budgets, GC stress and the stack-move stress; the control limits turn the pieces off one by one.
/// Mutants `rec_ret_wanted_unchecked`, `rec_ret_top_off`, `rec_cont_unroll_off` and `rec_side_call_skip`.
#[test]
fn r2_recursion_programs() {
    let on = "recnat=1,recany=1";
    let mut jobs = Vec::new();
    for p in ["r2_rec", "r2_rec_deep"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = on.into();
                jobs.push(j);
            }
            for lim in [
                "recnat=1",
                "recnat=1,recany=1",
                "recnat=1,recany=1,unroll=1",
                "recnat=1,recany=1,unroll=4,maxdepth=6",
                "recnat=1,recany=1,loopunroll=2",
                "recnat=1,recany=1,maxside=0",
                "recnat=0",
            ] {
                let mut j = Job::new(p, trace);
                j.limits = lim.into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = on.into();
                jobs.push(j);
            }
            if p == "r2_rec" {
                let mut j = Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
                j.limits = on.into();
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.limits == on
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.budget.is_none()
            && j.prog == "r2_rec"
        {
            assert!(
                o.stats.installed >= 3,
                "{}: traces installed: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                o.stats.entered >= 1,
                "{}: no trace entered: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// BUNDLE-E: the native-return guards of a frameful side trace (`rec_ret_guards`) keep the registers that hold the frames' function
/// slots (root cause of the old queens SEGV: they wrote `ci.previous` into a function slot through the start snapshot). The `contok`
/// test knob records continuation heads / native returns for the loop recursion of `be_queens` (the shape of awfy `queens`), with the
/// deterministic work gate off (`warm=0`, the failing configuration), in trace modes with lockstep, exit storms, budgets and GC
/// stress. Mutant `rec_ret_regs_unsaved`.
#[test]
fn be_native_return_guards_keep_registers() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for lim in [
            "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16,contok=1",
            "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16,contok=2",
            "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16,sidemin=4,contok=1",
            "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,contok=1",
            // controls: without `contok`, and without the side-trace room checks that the failing shapes need
            "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16",
            "recnat=1,recany=1,siderooms=0,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16,contok=1",
        ] {
            for (opts, budget) in [("", 0), ("+verify", 0), ("+exitstorm=3 +verify", 0), ("+exitstorm=7 +verify +randra=5", 0), ("", 17), ("-loop", 0)] {
                let mut j = Job::new("be_queens", trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = lim.into();
                jobs.push(j);
            }
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("be_queens", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = "recnat=1,recany=1,siderooms=1,nilstore=1,siderooms=1,typesides=1,ineffcover=1,maxside=16,contok=1".into();
            jobs.push(j);
        }
    }
    compare(jobs);
}

/// R2: side traces entered with virtual frames that need more stack / `CallInfo` room than the parent guarantees check it themselves
/// (`siderooms=1`; `siderooms=0` is the control that aborts them `Compile(side-room)`), in the trace modes with lockstep, exit storms,
/// budgets, GC stress and the stack-move stress. Mutant `side_room_check_skip`.
#[test]
fn r2_side_room_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
        ] {
            let mut j = Job::new("r2_side_room", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = "siderooms=1".into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("r2_side_room", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = "siderooms=1".into();
            jobs.push(j);
        }
        let mut j = Job::new("r2_side_room", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = "siderooms=1".into();
        jobs.push(j);
        let mut j = Job::new("r2_side_room", trace);
        j.limits = "siderooms=0".into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.limits == "siderooms=1"
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.budget.is_none()
        {
            assert!(
                o.stats.side_installed >= 2,
                "{}: side traces installed: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                !o.stats.aborts.contains_key("Compile(side-room)"),
                "{}: side-room aborts: {}",
                j.prog,
                o.stats.line()
            );
        }
        // Y4: the control without room checks at the start of a side trace refuses a frameful side trace that needs more room than its parent
        // checked (`Compile(side-room)`); that refusal is the install-time check the mutant `fside_room_ignored` removes (deterministic detector,
        // no reliance on the heap corruption the missing check causes at run time)
        if j.limits == "siderooms=0"
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.budget.is_none()
        {
            assert!(
                o.stats.side_aborts.contains_key("Compile(side-room)"),
                "{}: siderooms=0 refused no oversize frameful side trace: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// R2: stores into hash nodes that hold nil and nil stores to absent keys (`nilstore=1,siderooms=1`; `nilstore=0` is the control that aborts them
/// `Insert`) in the trace modes with lockstep, exit storms, budgets, GC stress and the stack-move stress. Mutants
/// `nilstore_newindex_skip`, `nilstore_meta_name_ok`.
#[test]
fn r2_nilstore_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
        ] {
            let mut j = Job::new("r2_nilstore", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = "nilstore=1,siderooms=1".into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("r2_nilstore", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = "nilstore=1,siderooms=1".into();
            jobs.push(j);
        }
        let mut j = Job::new("r2_nilstore", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = "nilstore=1,siderooms=1".into();
        jobs.push(j);
        let mut j = Job::new("r2_nilstore", trace);
        j.limits = "nilstore=0".into();
        jobs.push(j);
    }
    let res = compare(jobs);
    let (mut with, mut without) = (None, None);
    for (j, o) in &res {
        if j.trace == "always" && j.opts.is_empty() && j.env.is_empty() && j.budget.is_none() {
            if j.limits == "nilstore=1,siderooms=1" {
                with = Some(o.stats.aborts.get("Insert").copied().unwrap_or(0));
            } else {
                without = Some(o.stats.aborts.get("Insert").copied().unwrap_or(0));
            }
        }
    }
    assert!(with < without, "nilstore=1,siderooms=1 must abort `Insert` less often than nilstore=0: {with:?} vs {without:?}");
}

/// B2: scaled array addressing (`b2arr`), the metatable-free read of a non-nil array element (`b2mt`) and the known-tag store skip
/// (`b2tag`) in the trace modes with lockstep, exit storms, budgets, GC stress and the stack-move stress (K3b: `b2*` are hard-wired on).
/// Mutants `b2arr_no_not`, `b2arr_off8`, `b2mt_store`, `b2tag_alias_keep`.
#[test]
fn b2_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("b2_backend", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = String::new();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("b2_backend", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = String::new();
            jobs.push(j);
        }
        let mut j = Job::new("b2_backend", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = String::new();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits.is_empty() {
            assert!(
                o.stats.installed >= 3 && o.stats.entered >= 1,
                "b2_backend: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// B4-1: tag-only stores of nil / false / true (`nopay`) in the trace modes with lockstep, exit storms, budgets, GC stress and the
/// stack-move stress (K3b: `nopay` is hard-wired on). Mutant `nopay_all_types`.
#[test]
fn b4_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("b4_backend", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("b4_backend", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = ON.into();
            jobs.push(j);
        }
        let mut j = Job::new("b4_backend", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 3 && o.stats.entered >= 1,
                "b4_backend: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// B4-4: magic-number `//` and `%` by constants (`divmagic`) in the trace modes with lockstep, exit storms, budgets, GC stress and the
/// stack-move stress (K3b: `divmagic` is hard-wired on). Mutants `divmagic_no_sign`, `divmagic_shift_off`.
#[test]
fn b4_div_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("b4_div", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("b4_div", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = ON.into();
            jobs.push(j);
        }
        let mut j = Job::new("b4_div", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 3 && o.stats.entered >= 1,
                "b4_div: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// B5-1: allocation sinking of tables with a metatable (`sinkmt`) and in side / stitched traces (`sinkside`) in the trace modes with
/// lockstep, exit storms, budgets, GC stress and the stack-move stress; the knobs off are the controls. Mutants `sinkmt_gc_guard_skip`,
/// `sinkmt_no_mt_materialise`, `sinkside_dead_all`, `sinkmt_never` (its detector asserts that tables were sunk).
#[test]
fn b5_sink_programs() {
    const ON: &str = "sinkmt=1,sinkside=1";
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("b5_sink", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            for gm in ["gen", "inc"] {
                let mut j = Job::new("b5_sink", trace)
                    .env("RUSTYNAIL_GC_STRESS", gc)
                    .env("TR_CORE_GCMODE", gm);
                j.limits = ON.into();
                jobs.push(j);
            }
        }
        let mut j = Job::new("b5_sink", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
        for l in ["sinkmt=0,sinkside=0", "sinkmt=1", "sinkside=1"] {
            let mut j = Job::new("b5_sink", trace);
            j.limits = l.into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 3 && o.stats.entered >= 1,
                "b5_sink: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// B5-1: the knobs do what they say: with `sinkmt` the `setmetatable({...}, V)` temporaries of `b5_sink` are sunk (more tables than without),
/// `sinkside` adds the tables of side traces. Detector of the mutant `sinkmt_never`.
#[test]
fn b5_sink_counts() {
    let mk = |l: &str| {
        let mut j = Job::new("b5_sink", "always").opts("+verify");
        j.limits = l.into();
        j
    };
    let res = compare(vec![
        mk("sinkmt=0,sinkside=0"),
        mk("sinkmt=1,sinkside=0"),
        mk("sinkmt=1,sinkside=1"),
    ]);
    let n: Vec<u64> = res.iter().map(|(_, o)| o.stats.sunk).collect();
    assert!(n[1] > n[0], "sinkmt sank nothing: {n:?}");
    assert!(n[2] >= n[1], "sinkside sank fewer tables: {n:?}");
}

/// B4-3: `HREFK` + `HLOAD` with one 16-bit tag compare (`hkfuse`, needs `b2hk`) in the trace modes with lockstep, exit storms, budgets,
/// GC stress and the stack-move stress (K3b: `hkfuse` is hard-wired on). Mutant `hkfuse_tag_skip` (a `hkfuse_snap_any` mutant would be equivalent: the guards of one field read share their snapshot).
#[test]
fn b4_hk_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("b4_hk", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("b4_hk", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = ON.into();
            jobs.push(j);
        }
        let mut j = Job::new("b4_hk", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 3 && o.stats.entered >= 1,
                "b4_hk: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// J1-1: the `HREFK`+`HLOAD` tag fusion fires (it stopped silently once, when the type guard got its own snapshot): the installed traces of
/// field-reading programs contain fused pairs (stat line `fused HREFK+HLOAD tag checks`), also with two exits (`j1_hk`: the type guard of
/// the `HLOAD` has a snapshot of its own). Detector of the mutant `hkfuse_split_never`.
#[test]
fn j1_hkfuse_fires() {
    let mut jobs = Vec::new();
    for prog in ["b4_hk", "j1_hk", "be_queens"] {
        for l in [""] {
            let mut j = Job::new(prog, "on");
            j.limits = l.into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog != "be_queens" {
            assert!(
                o.stats.hk_fused >= 3,
                "{}: the fusion does not fire: {} (fused {})",
                j.prog,
                o.stats.line(),
                o.stats.hk_fused
            );
            assert!(
                o.stats.hk_split >= 1,
                "{}: no fused pair with two exits (the typesides split): {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// The modes every BUNDLE-J2 program runs in: lockstep, exit storms, small budgets, the stack-move stress and both GC stress modes.
fn j2_jobs(prog: &str, limits: &str, traces: &[&str]) -> Vec<Job> {
    let mut jobs = Vec::new();
    for &trace in traces {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 17),
            ("-loop -fwd -cse -sink", 0),
        ] {
            let mut j = Job::new(prog, trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = limits.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = limits.into();
            jobs.push(j);
        }
        let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = limits.into();
        jobs.push(j);
    }
    jobs
}

/// J2-2: per-site deeper inlining. A loop recording that stops at the recursion limit is re-recorded once with `unroll`/`maxdepth` raised
/// to `deep` (and its side recordings with it); a site whose deep recording stops at the limit again keeps the normal penalties (no
/// re-record loop); the IR bound `deepins` stops a deep recording of a big call tree (`TooLong`). Detectors of the mutants `deep_never`,
/// `deep_every_time`, `deep_limits_ignored`, `deep_ins_unbounded`.
#[test]
fn j2_deep_programs() {
    const ON: &str = "deep=8,deepins=300";
    let mut jobs = Vec::new();
    jobs.extend(j2_jobs("j2_deep", ON, &["always", "on2"]));
    jobs.extend(j2_jobs("j2_deep", "deep=0", &["always"]));
    jobs.extend(j2_jobs(
        "j2_deep",
        "deep=8,deepins=300,recnat=1,recany=1",
        &["always"],
    ));
    let res = compare(jobs);
    let pick = |trace: &str, limits: &str| -> TStats {
        res.iter()
            .find(|(j, _)| {
                j.prog == "j2_deep"
                    && j.trace == trace
                    && j.opts.is_empty()
                    && j.budget.is_none()
                    && j.env.is_empty()
                    && j.limits == limits
            })
            .unwrap_or_else(|| panic!("no result for {trace} {limits}"))
            .1
            .stats
            .clone()
    };
    let on = pick("always", ON);
    let off = pick("always", "deep=0");
    assert_eq!(
        (off.deep_sites, off.deep_recs),
        (0, 0),
        "deep stats without the knob: {}",
        off.line()
    );
    assert!(
        on.deep_sites >= 1 && on.deep_recs >= 2,
        "no site is re-recorded deeper: {}",
        on.line()
    );
    // once per site: the program has six loop heads, so at most six re-records (one each), never a chain of them (the third loop recurses
    // deeper than any limit: its deep recording stops at the limit again and keeps the normal penalties)
    assert!(
        on.deep_sites <= 6,
        "a site is re-recorded more than once: {}",
        on.line()
    );
    // the deep recording is installed (the loop trace of the first loop) and the program runs on traces
    assert!(
        on.installed >= off.installed,
        "fewer traces with the deep limits: {} vs {}",
        on.line(),
        off.line()
    );
    // the IR bound: with a tiny bound no deep recording completes (TooLong), with the default bound one does
    let mut j = Job::new("j2_deep", "always");
    j.limits = "deep=8,deepins=40".into();
    let tiny = compare(vec![j]).remove(0).1.stats;
    assert!(
        tiny.aborts.get("TooLong").copied().unwrap_or(0) >= 1,
        "the IR bound of a deep recording does not fire: {}",
        tiny.line()
    );
    assert!(tiny.deep_sites <= 6, "{}", tiny.line());
}

/// P2 bundle 3: the loop-top diet (`budsub`: `sub r, n; jl` with a refunding stub, `pollr15`: the poll word addressed from `r15`, `fpool`: double constants from
/// the per-state pool and `x * 2.0` as `x + x`) in the trace modes with lockstep, exit storms, budgets (3 and 17), GC stress, the stack-move stress and the emulator;
/// `pollr15=0` and the shipped value are the controls (K3b: `budsub` and `fpool` are hard-wired on). Mutants `budget_refund_skip`, `budget_refund_short`, `poll_r15_disp_off`, `fpool_bits_off`, `mul2_any_const`,
/// `loopdiet_default_ignored` (the hand tests of `tr_asm` and `tr_poll` also catch them).
#[test]
fn p2_loopdiet_programs() {
    const ON: &str = "pollr15=1";
    let mut jobs = Vec::new();
    for prog in ["p2_fconst", "float_poly", "special_floats", "b4_poll"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("+emurun +verify", 0),
                ("-fwd -cse -loop -sink", 0),
                ("+membudget +verify", 0),
            ] {
                let mut j = Job::new(prog, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = ON.into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = ON.into();
                jobs.push(j);
            }
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
            j.limits = ON.into();
            jobs.push(j);
            for l in ["pollr15=0", ""] {
                let mut j = Job::new(prog, trace);
                j.limits = l.into();
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "p2_fconst"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits == ON
        {
            assert!(
                o.stats.installed >= 2 && o.stats.entered >= 1,
                "p2_fconst: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// P2 bundle 2: upvalue / closure specialisation (`uvsep`: upvalue cells recorded as different `UpVal`s are disjoint for forwarding under run-time `NE` guards)
/// in the trace modes with lockstep, exit storms, budgets (3 and 17),
/// GC stress, the stack-move stress and the emulator (K3b: `uvsep` is hard-wired on). Mutants `uvsep_no_guard`, `uvsep_first_guard_only`,
/// (the hand tests of `tr_loop` and `tr_policy` also catch them).
#[test]
fn p2_uvsep_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for prog in ["p2_uvsep", "p2_uvroll", "upval_acc", "be_queens"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("+emurun +verify", 0),
                ("-fwd -cse -loop -sink", 0),
            ] {
                let mut j = Job::new(prog, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = ON.into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = ON.into();
                jobs.push(j);
            }
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
            j.limits = ON.into();
            jobs.push(j);
            for l in ["maxside=0", "nest=0"] {
                let mut j = Job::new(prog, trace);
                j.limits = l.into();
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if (j.prog == "p2_uvsep" || j.prog == "p2_uvroll")
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits == ON
        {
            assert!(
                o.stats.installed >= 2 && o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// P2 bundle 1: `rbp` / `r14` as allocatable trace registers (`xregs`) in the trace modes with lockstep, exit storms, budgets (3 and 17), GC stress, the stack-move stress,
/// the emulator and randomised register allocation; `xregs=0` is the control, each bit is also run alone. Mutants `xregs_default_ignored`, `xregs_no_reloadk`,
/// `xregs_reloadk_partial`, `xregs_rec_r14` (the hand tests of `tr_asm` / `tr_ra` also catch them).
#[test]
fn p2_xregs_programs() {
    const ON: &str = "xregs=3";
    let mut jobs = Vec::new();
    for prog in [
        "p2_xregs",
        "int_rec2",
        "be_queens",
        "b4_div",
        "rec_fib",
        "field_rw",
        "lib_str",
    ] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("+verify +randra=11", 0),
                ("+verify +randra=23", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("+emurun +verify", 0),
                ("-fwd -cse -loop -sink", 0),
                ("+membudget +verify", 0),
            ] {
                let mut j = Job::new(prog, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = ON.into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = ON.into();
                jobs.push(j);
            }
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
            j.limits = ON.into();
            jobs.push(j);
            // every exit restored by its stub (no side traces): the frame-rebuilding stubs jump into tier 1 through `JmpR` registers other than `V7`
            for l in ["xregs=0", "xregs=1", "xregs=2", "xregs=3,maxside=0"] {
                let mut j = Job::new(prog, trace);
                j.limits = l.into();
                jobs.push(j);
            }
            for opts in [
                "+exitstorm=3 +verify",
                "+exitstorm=5 +verify +randra=9",
                "+verify +randra=31",
            ] {
                let mut j = Job::new(prog, trace).opts(opts);
                j.limits = "xregs=3,maxside=0".into();
                jobs.push(j);
            }
        }
    }
    // native recursion (function-entry traces with a native frame push, which must not allocate `r14`)
    const XREC: &str = "recnat=1,recany=1,fnroot=1,fnmin=4,unroll=4,maxdepth=6,siderooms=1";
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("+verify +randra=11", 0),
            ("", 3),
            ("", 17),
            ("+emurun +verify", 0),
        ] {
            for xr in ["xregs=3", "xregs=0"] {
                let mut j = Job::new("p2_xregs_rec", trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = format!("{xr},{XREC}");
                jobs.push(j);
            }
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("p2_xregs_rec", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = format!("xregs=3,{XREC}");
            jobs.push(j);
        }
        let mut j = Job::new("p2_xregs_rec", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = format!("xregs=3,{XREC}");
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "p2_xregs"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits == ON
        {
            assert!(
                o.stats.installed >= 2 && o.stats.entered >= 1,
                "p2_xregs: traces did not run: {}",
                o.stats.line()
            );
        }
    }
}

/// F2: library calls recorded as trace constructs (string comparisons, getmetatable, rawget / rawset, assert, string.byte, integer math.random)
/// in the trace modes with lockstep, exit storms, budgets, GC stress and the stack-move stress (K3b: the `f2*` knobs are hard-wired on; the jobs run the shipped configuration).
/// Mutants `strcmp_lt_as_le`, `strcmp_guard_skip`, `getmt_null_skip`, `getmt_protected_skip`, `assert_extra_dropped`, `rawset_tbar_skip`,
/// `rawset_result_nil`.
#[test]
fn f2_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for (prog, trace) in ["f2_lib", "f2_byte"]
        .into_iter()
        .flat_map(|p| ["always", "on2", "on"].map(move |t| (p, t)))
    {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new(prog, trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = ON.into();
            jobs.push(j);
        }
        let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
    // the recorders replace aborts: the programs have (almost) no `CFunc` / `Nyi` aborts (K3b: the knob-off control is gone; the figure is printed)
    let mut on = 0u64;
    for (j, o) in &res {
        if j.trace == "always" && j.opts.is_empty() && j.env.is_empty() && j.budget.is_none() {
            on += o
                .stats
                .aborts
                .iter()
                .filter(|(k, _)| k.starts_with("CFunc") || k.starts_with("Nyi"))
                .map(|(_, v)| *v)
                .sum::<u64>();
        }
    }
    eprintln!("f2_programs: CFunc / Nyi aborts {on}");
    assert!(
        on <= 100,
        "the f2 recorders must remove CFunc / Nyi aborts: {on}"
    );
}

/// BUNDLE-G1 items 4 and 5: `string.format("%s", s)` folded to `s` (K3b: `g1fmt`, hard-wired on) and the inline long-string equality (`asm::streq_fast`) in every trace mode with lockstep, exit storms, budgets, GC stress and the stack-move stress. Mutants `g1fmt_fmt_guard_skip`, `g1fmt_result_fmt`, `streq_inline_len_diff_eq`, `streq_inline_tail_ne_eq`, `streq_inline_tail_eq`.
#[test]
fn g1_programs() {
    const ON: &str = "";
    let mut jobs = Vec::new();
    for (prog, trace) in ["g1_fmt", "g1_streq"]
        .into_iter()
        .flat_map(|p| ["always", "on2", "on"].map(move |t| (p, t)))
    {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
        ] {
            let mut j = Job::new(prog, trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = ON.into();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new(prog, trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = ON.into();
            jobs.push(j);
        }
        let mut j = Job::new(prog, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits == ON {
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// C2: tier-1 comparisons of non-numbers / `SETUPVAL` of collectable values (`jit_cmp`, `jit_setupval`; no knob: the controls are the
/// jit-off twin and the trace modes) in every trace mode, with exit storms, budgets, GC stress and the stack-move stress.
/// Mutants `c2cmp_swap`, `c2cmp_exit_skip`, `c2cmp_eq_mm_skip`, `c2setupval_barrier_skip`.
#[test]
fn c2_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("c2_tier1", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("c2_tier1", trace).env("RUSTYNAIL_GC_STRESS", gc));
        }
        jobs.push(Job::new("c2_tier1", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-B: tier-1 comparisons whose `__lt` / `__le` / `__eq` is a Lua function (the metamethod frame is pushed by `jit_cmp`, its `RETURN`
/// finishes the comparison by `jit_mm_ret` and a resume entry at the `JMP`): program `bb_cmpmm` against the jit-off run in every trace mode,
/// with exit storms, budgets, GC stress and the stack-move stress. Mutants `mmret_nc_skip`, `mmret_flag_false`, `mmcmp_savedpc_skip`
/// (and `c2cmp_swap`, which also swaps the continuation).
#[test]
fn bb_cmpmm_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("bb_cmpmm", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("bb_cmpmm", trace).env("RUSTYNAIL_GC_STRESS", gc));
        }
        jobs.push(Job::new("bb_cmpmm", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-B item 2 (`tset`, bit 2 = the nil-slot stores): the tier-1 stores into nil-valued nodes and empty array slots of tables with a metatable (limit `nilset`, struct default
/// 0): programs `ba_nilset` and `r2_nilstore` against the interpreter run, in every trace mode, with exit storms, budgets, GC stress and the
/// stack-move stress, with the knob on and (for the control) off. Programs `bb_tset` (string and integer-hash keys) too. Mutants `nilset_flags_skip`,
/// `nilset_mt_skip`, `nilset_barrier_skip`, `nilset_arr_mt_skip`, `tset_str_tag_skip`, `tset_int_key_unchecked`.
#[test]
fn bb_tset_programs() {
    let mut jobs = Vec::new();
    for p in ["ba_nilset", "r2_nilstore", "bb_tset", "g1_tget"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc));
            }
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
        }
    }
    for j in jobs.iter_mut() {
        j.limits = "tset=15".into();
    }
    for p in ["ba_nilset", "r2_nilstore", "bb_tset", "g1_tget"] {
        let mut off = Job::new(p, "always").opts("+verify");
        off.limits = "tset=0".into();
        jobs.push(off);
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// T3: the inline first `__index` level of the tier-1 `GETFIELD` / `SELF` / `GETTABUP` templates (limit `t3idx`, struct default 0): every
/// receiver-miss shape of program `t3_index` against the interpreter run, in every trace mode, with exit storms, budgets, GC stress and
/// the stack-move stress, with the knob on and (for the control) off. Mutants `t3idx_nil_hit`, `t3idx_use_mt`.
#[test]
fn t3_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("t3_index", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("t3_index", trace).env("RUSTYNAIL_GC_STRESS", gc));
        }
        jobs.push(Job::new("t3_index", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    for j in jobs.iter_mut() {
        j.limits = "t3idx=1".into();
    }
    let mut off = Job::new("t3_index", "always").opts("+verify");
    off.limits = "t3idx=0".into();
    jobs.push(off);
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-T1I: the inline built-ins of the tier-1 `CALL` template (`type`, `math.floor/abs/max/min/sqrt`, `string.byte/len`, `select('#')`,
/// `rawlen`): normal and abnormal arguments, replaced globals / fields / aliases in the middle of hot loops, hooks, other argument counts,
/// multiple results and the argument errors against the interpreter run, in every trace mode (the knob is on with traces only), with exit
/// storms, budgets, GC stress in both modes and the stack-move stress; the knob off and single bits are the controls.
/// Mutants `t1i_type_ident_skip`, `t1i_lcf_ident_skip`, `t1i_floor_inexact`, `t1i_minmax_le`, `t1i_byte_lo_skip`, `t1i_abs_float_sign`,
/// `t1i_select_count`, `t1i_rawlen_next_skip`, `t1i_sqrt_int_bits`, `t1i_type_index`.
#[test]
fn t1i_inline_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("t1i_inline", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("t1i_inline", trace).env("RUSTYNAIL_GC_STRESS", gc));
            jobs.push(
                Job::new("t1i_inline", trace)
                    .env("RUSTYNAIL_GC_STRESS", gc)
                    .env("RUSTYNAIL_GC_MODE", "inc"),
            );
        }
        jobs.push(Job::new("t1i_inline", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    for j in jobs.iter_mut() {
        j.limits = "t1inline=1023".into();
    }
    // (BUNDLE-M: floats whose floor is not an integer: mutant `t1i_floor_inexact`)
    for trace in ["always", "on2"] {
        for opts in ["", "+verify"] {
            let mut j = Job::new("t1i_floor2", trace).opts(opts);
            j.limits = "t1inline=1023".into();
            jobs.push(j);
        }
    }
    // controls: the knob off, and every bit alone
    for l in [
        "t1inline=0",
        "t1inline=1",
        "t1inline=2",
        "t1inline=4",
        "t1inline=8",
        "t1inline=16",
        "t1inline=32",
        "t1inline=64",
        "t1inline=128",
        "t1inline=256",
        "t1inline=512",
    ] {
        let mut off = Job::new("t1i_inline", "always").opts("+verify");
        off.limits = l.into();
        jobs.push(off);
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-K (B): the tier-1 `SELF` on a string receiver (`strself`): every change of the string library / metatable between calls of a hot
/// loop against the interpreter run, in every trace mode, with exit storms, budgets, GC stress and the stack-move stress (K3b: `strself` is hard-wired on; `t3idx=0` is the control without the string metatable address).
/// Mutants `strself_tag_skip`, `strself_mt_skip`, `strself_nil_hit`.
#[test]
fn k_strself_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("k_strself", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("k_strself", trace).env("RUSTYNAIL_GC_STRESS", gc));
        }
        jobs.push(Job::new("k_strself", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    for j in jobs.iter_mut() {
        j.limits = "t3idx=2".into();
    }
    // (BUNDLE-M: the receiver of a hot `SELF` site that is sometimes not a string: mutant `strself_tag_skip`)
    for trace in ["always", "on2", "on"] {
        for (opts, lim) in [
            ("", "t3idx=2"),
            ("+verify", "t3idx=2"),
            ("+verify", ""),
            ("+exitstorm=3 +verify", ""),
        ] {
            jobs.push(Job::new("k_strself2", trace).opts(opts).limits_s(lim));
        }
    }
    for l in ["t3idx=0"] {
        let mut off = Job::new("k_strself", "always").opts("+verify");
        off.limits = l.into();
        jobs.push(off);
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-L (D1): collectable stores into closed upvalues (aborted: the upvalue-barrier recording was removed in K1), absent-key loads (integer and string
/// keys, nil-valued nodes, metatables added in the middle, rehash) and CONCAT-built keys in every trace mode, with exit storms, budgets, GC stress (incremental
/// and generational, with the heap verifier) and the stack-move stress; `dynmask=1` (dynamic keys with the absence walk) and the control without it.
/// (The `HREF_ABS` walk is `dynmask`'s: the mutant `habs_found_ignored` is detected by `dm_programs`.)
#[test]
fn l_d1_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("l_d1", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            jobs.push(Job::new("l_d1", trace).env("RUSTYNAIL_GC_STRESS", gc));
            jobs.push(
                Job::new("l_d1", trace)
                    .env("RUSTYNAIL_GC_STRESS", gc)
                    .env("TR_CORE_GCMODE", "gen"),
            );
        }
        jobs.push(
            Job::new("l_d1", trace)
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
        jobs.push(Job::new("l_d1", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
    }
    for j in jobs.iter_mut() {
        j.limits = "dynmask=1".into();
    }
    for l in ["dynmask=0", "dynmask=1,lockstep=1"] {
        let mut off = Job::new("l_d1", "always").opts("+verify");
        off.limits = l.into();
        jobs.push(off);
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// DYNMASK: dynamic short-string keys with the node count and node vector read at run time (`HREFV` `HREF_DYN`), nil-miss loads (`HREF_NIL`)
/// and `NEWREF` for absent dynamic stores: a table that rehashes between lookups of one trace, tables of different sizes in one loop (the
/// empty table's dummy node included), `__index` / `__newindex` chains that change in the middle, `rawget`, long strings and non-string keys
/// in a dynamic-key loop, nil / NaN keys (the same errors), array parts, dead keys, concatenation-built keys, in every trace mode with
/// verification, exit storms, budgets, GC stress (incremental and generational, with the heap verifier) and the stack-move stress; the knob off
/// is the control. Mutants `dynmask_mask_off_by_one`, `dynmask_cse_merge`, `dynmask_nil_store`,
/// `dynmask_nil_meta_skip`.
#[test]
fn dm_programs() {
    let mut jobs = Vec::new();
    for p in ["dm_dynkey", "dm_growth"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("-fwd -cse -sink", 0),
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc));
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", gc)
                        .env("TR_CORE_GCMODE", "gen"),
                );
            }
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1"),
            );
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
        }
    }
    for j in jobs.iter_mut() {
        j.limits = "dynmask=1".into();
    }
    let mut more = Vec::new();
    for p in ["dm_dynkey", "dm_growth"] {
        for l in ["dynmask=1,lockstep=1", "dynmask=0"] {
            for opts in ["+verify", "+exitstorm=3 +verify"] {
                let mut j = Job::new(p, "always").opts(opts);
                j.limits = l.into();
                more.push(j);
            }
        }
    }
    jobs.extend(more);
    // the shipped process defaults of every other knob (nil-valued stores, header-safe library calls, 64-bit key compares, ...)
    for p in ["dm_dynkey", "dm_growth"] {
        for trace in ["always", "on2"] {
            for opts in [
                "",
                "+verify",
                "+exitstorm=3 +verify",
                "+exitstorm=7 +verify +randra=5",
            ] {
                let mut j = Job::new(p, trace).opts(opts).env("TR_CORE_PROCESS", "1");
                j.limits = "dynmask=1".into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(p, trace)
                    .env("TR_CORE_PROCESS", "1")
                    .env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = "dynmask=1".into();
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "dm_dynkey"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.limits == "dynmask=1"
            && j.env.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// DYNMASK: a table that grows between the lookups of a loop does not make the loop exit per rehash (the node-count guard of the older form
/// did: a side trace chain, then a blacklisted exit and an exit per iteration). Detector of `dynmask_mask_off_by_one` (a key at a main position
/// the wrong mask does not reach is a run-time miss).
#[test]
fn dm_growth_exits_are_bounded() {
    let mut off = Job::new("dm_growth", "always").opts("+verify");
    // `typesides=0`: the bound below is about the rehash exits; with the shipped `typesides` the value-type flips of the program (nil to number
    // at the first hit of each loop) get side traces whose hops are counted as exits as well
    off.limits = "dynmask=0,typesides=0".into();
    let mut on = Job::new("dm_growth", "always").opts("+verify");
    on.limits = "dynmask=1,typesides=0".into();
    let res = compare(vec![off, on]);
    let (e0, e1) = (res[0].1.stats.exits, res[1].1.stats.exits);
    assert!(
        res[1].1.stats.installed >= 1,
        "no trace: {}",
        res[1].1.stats.line()
    );
    assert!(
        e1 <= e0,
        "dynmask=1 exits more ({e1}) than dynmask=0 ({e0}): {}",
        res[1].1.stats.line()
    );
    assert!(
        e1 < 6000,
        "too many exits with dynmask=1: {}",
        res[1].1.stats.line()
    );
}

/// P2: the tree-aware ineffective-trace rule (`ineffcover`) and side traces on every hot type-guard exit (`typesides`): the scheduler
/// loop (many exit sites, each with a side trace), the type-flip phases of rolled loops and the exit storm that must terminate, against
/// the interpreter run in every trace mode, with exit storms, budgets, GC stress and the stack-move stress; the knobs off are the control.
/// Mutants `ineff_cover_ignored`, `ineff_persist_never`, `typesides_ignored`.
#[test]
fn p2_programs() {
    let mut jobs = Vec::new();
    for p in ["p2_scheduler", "p2_type_flip", "p2_storm"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc));
            }
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
        }
    }
    for j in jobs.iter_mut() {
        j.limits = "ineffcover=1,typesides=1".into();
    }
    for p in ["p2_scheduler", "p2_type_flip", "p2_storm"] {
        for l in [
            "ineffcover=0,typesides=0",
            "ineffcover=1,typesides=0",
            "ineffcover=0,typesides=1",
        ] {
            let mut j = Job::new(p, "always").opts("+verify");
            j.limits = l.into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// P2: the scheduler loop keeps its tree: with `ineffcover` no root is flushed as ineffective and side traces are installed.
/// Mutant `ineff_cover_ignored`.
#[test]
fn p2_scheduler_tree_is_kept() {
    let mut on = Job::new("p2_scheduler", "always");
    on.limits = "ineffcover=1,typesides=1".into();
    let r = run_all(vec![on]).remove(0).1;
    assert!(r.ok, "{}", r.stderr);
    assert_eq!(r.stats.ineffective, 0, "p2_scheduler: {}", r.stats.line());
    assert!(
        r.stats.side_installed >= 2,
        "p2_scheduler: {}",
        r.stats.line()
    );
}

/// P2: a hot type-guard exit of a rolled root gets a side trace instead of flushing the family (`typesides`); the control flushes
/// (`Rerec`). Mutant `typesides_ignored`.
#[test]
fn p2_type_guard_exits_get_side_traces() {
    let mut on = Job::new("p2_type_flip", "always");
    on.limits = "ineffcover=1,typesides=1".into();
    let mut off = Job::new("p2_type_flip", "always");
    off.limits = "ineffcover=1,typesides=0".into();
    let mut res = run_all(vec![on, off]);
    let off = res.remove(1).1;
    let on = res.remove(0).1;
    assert!(on.ok && off.ok, "{} {}", on.stderr, off.stderr);
    assert!(
        on.stats.side_installed > off.stats.side_installed,
        "on: {} off: {}",
        on.stats.line(),
        off.stats.line()
    );
}

/// P2: a persistent type change at a peeled root's guard (the `ineff_marking` pattern) is still flushed as ineffective with
/// `ineffcover` on and the head ends Black: the coverage rule only spares trees whose exits are spread over several sites.
/// Mutant `ineff_persist_never`.
// needs the mutant `forloop_step_always_lit` (feature `tr_mutants`), see `ineffective_trace_rule`
#[cfg(feature = "tr_mutants")]
#[test]
fn p2_persistent_change_is_still_flushed() {
    for trace in ["always", "on2"] {
        let mut muts = std::env::var("RUSTYNAIL_JIT_MUT").unwrap_or_default();
        if !muts.is_empty() {
            muts.push(',');
        }
        muts.push_str("forloop_step_always_lit");
        let mut j = Job::new("ineff_marking", trace).env("RUSTYNAIL_JIT_MUT", &muts);
        j.limits = "ineffcover=1,typesides=1,hoptop=98".into();
        let r = run_all(vec![j]).remove(0).1;
        assert!(r.ok, "ineff_marking: {}", r.stderr);
        assert!(
            r.stats.ineffective >= 1,
            "ineff_marking [{trace}]: the rule did not fire: {}",
            r.stats.line()
        );
        assert!(
            r.stats.blacklisted >= 1,
            "ineff_marking [{trace}]: {}",
            r.stats.line()
        );
        assert!(
            r.stats.exits <= 64 * 1024,
            "ineff_marking [{trace}]: too many exits: {}",
            r.stats.line()
        );
    }
}

/// P-M3: the native self-recursion program in every mode with the lockstep, exit storms, small budgets, GC stress and the limits
/// that turn the feature off or change the unroll depth (`recnat` is off by default: the jobs turn it on).
#[test]
fn pm3_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        jobs.push(
            Job::new("pm3_rec2", trace)
                .opts("+verify")
                .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
        );
        jobs.push(Job::new("pm3_rec", trace).opts("+verify"));
        jobs.push(Job::new("pm3_rec", trace).opts("+exitstorm=3"));
        jobs.push(Job::new("pm3_rec", trace).opts("+exitstorm=1 +verify"));
        jobs.push(Job::new("pm3_rec", trace).opts("+exitstorm=7 +verify +randra=3"));
        jobs.push(Job::new("pm3_rec", trace).budget(17));
        jobs.push(Job::new("pm3_rec", trace).budget(1));
    }
    jobs.push(Job::new("pm3_rec", "always").opts("-loop -fwd"));
    // BUNDLE-M: the `CallInfo.top` of a natively pushed frame (`__citop`, mutant `rec_push_top_off`)
    for trace in ["always", "on2"] {
        jobs.push(Job::new("bm_rec_top", trace).opts("+verify"));
        jobs.push(Job::new("bm_rec_top", trace).opts("+exitstorm=3"));
    }
    for gc in ["1", "2"] {
        let mut j = Job::new("pm3_rec", "always").opts("+verify");
        j.env.push(("RUSTYNAIL_GC_STRESS".into(), gc.into()));
        jobs.push(j);
    }
    for l in [
        "recnat=1,fnroot=1",
        "recnat=1,unroll=4,maxdepth=6",
        "recnat=0",
    ] {
        let mut j = Job::new("pm3_rec", "always").opts("+verify");
        j.limits = l.into();
        jobs.push(j);
    }
    for j in jobs.iter_mut().filter(|j| j.limits.is_empty()) {
        j.limits = "recnat=1".into();
    }
    compare(jobs);
}

/// BUNDLE-K L1 (`lazyci`): the recursion programs and the observers at depth (traceback, getinfo, getlocal, errors through pcall / xpcall, hooks,
/// collector, coroutine yields, stack overflow, roots entered from C) with lazy frames in every trace mode, with exit storms (every guard fails
/// every N-th time: a chain is completed at a different depth each time), small budgets, GC stress and the limits that change the unroll depth.
/// The mutants `lazy_*` break the completion of a chain, the region root or the native return; this family catches them.
#[test]
fn bk_lazy_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for p in ["pm3_rec", "bk_obs1", "bk_obs2", "r2_rec"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=1 +verify"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).budget(1));
        }
    }
    jobs.push(Job::new("px_rec_landing", "always").opts("+verify"));
    jobs.push(Job::new("px_rec_landing", "always").opts("+exitstorm=3"));
    jobs.push(
        Job::new("bk_obs1", "always")
            .opts("+verify")
            .env("RUSTYNAIL_GC_STRESS", "2"),
    );
    jobs.push(
        Job::new("bk_obs1", "always")
            .opts("+verify")
            .env("RUSTYNAIL_GC_STRESS", "1"),
    );
    for l in ["unroll=4,maxdepth=6", "fnroot=1", "lazyci=0"] {
        let mut j = Job::new("bk_obs1", "always").opts("+verify");
        j.limits = format!("recnat=1,lazyci=1,{l}").replace("lazyci=1,lazyci=0", "lazyci=0");
        jobs.push(j);
    }
    for j in jobs.iter_mut().filter(|j| j.limits.is_empty()) {
        j.limits = "recnat=1,lazyci=1".into();
    }
    compare(jobs);
}

/// BUNDLE-L (`aidx`, `bhoist`): the array-loop programs (observers, boundaries, matrices) against the interpreter in every checking mode, with each
/// knob alone and both. The mutants `aidx_*` / `bhoist_*` break the complemented index, its displacement, the hoisted guards or their coverage.
#[test]
fn bl_programs() {
    let mut jobs = Vec::new();
    // (BUNDLE-M: `bhoist` is ON by default, so `aidx` alone needs the explicit `bhoist=0`)
    for lim in ["aidx=1,bhoist=0", "aidx=1", "bhoist=1", "aidx=1,bhoist=1"] {
        for trace in ["always", "on2"] {
            for p in ["bl_obs1", "bl_oob", "bl_mat", "bl_off"] {
                for opts in [
                    "",
                    "+verify",
                    "+exitstorm=3",
                    "+exitstorm=1 +verify",
                    "+randra=5 +verify",
                ] {
                    let mut j = Job::new(p, trace).opts(opts);
                    j.limits = lim.into();
                    jobs.push(j);
                }
            }
        }
        for p in ["bl_obs1", "bl_oob", "bl_mat", "bl_off"] {
            let mut j = Job::new(p, "always")
                .opts("+verify")
                .env("RUSTYNAIL_GC_STRESS", "2");
            j.limits = lim.into();
            jobs.push(j);
            let mut j = Job::new(p, "always").budget(17);
            j.limits = lim.into();
            jobs.push(j);
        }
    }
    compare(jobs);
}

/// BUNDLE-L: the traces of the array programs are installed and entered with both knobs on, and the hoisted guards exist (`ir.hoisted`).
#[test]
fn bl_programs_enter_traces() {
    let mut j = Job::new("bl_mat", "always").opts("+verify");
    j.limits = "aidx=1,bhoist=1".into();
    let res = compare(vec![j]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 2 && o.stats.entered >= 1,
            "{}: no trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// BUNDLE-K L1: the method / global recursion programs of R2 (`recany`: `SELF` and `GETTABUP` callees, `RETURN0`, call sites that want 0 / 1 / 2 / all results) with
/// lazy frames; `pm3_rec` has `pcall(fib, ...)` roots (a `CIST_FRESH` root frame must not return natively: the mutant `lazy_ret_root_unchecked`).
#[test]
fn bk_lazy_r2_programs() {
    let on = "recnat=1,recany=1,lazyci=1";
    let mut jobs = Vec::new();
    for p in ["r2_rec", "pm3_rec"] {
        for trace in ["always", "on2"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                if budget != 0 {
                    j = j.budget(budget);
                }
                j.limits = on.into();
                jobs.push(j);
            }
            for lim in [
                "recnat=1,recany=1,lazyci=1,unroll=4,maxdepth=6",
                "recnat=1,recany=1,lazyci=1,maxside=0",
                "recnat=1,recany=1,lazyci=1,unroll=1",
            ] {
                let mut j = Job::new(p, trace);
                j.limits = lim.into();
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                let mut j = Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc);
                j.limits = on.into();
                jobs.push(j);
            }
        }
    }
    compare(jobs);
}

/// BUNDLE-K L1: the lazy traces run (the recursion root is entered and the chain is completed at exits): the observer program installs traces and
/// reports exits at depth.
#[test]
fn bk_lazy_programs_enter_traces() {
    let mut j = Job::new("bk_obs1", "always").opts("+verify");
    j.limits = "recnat=1,lazyci=1".into();
    let res = compare(vec![j]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 4 && o.stats.entered >= 1 && o.stats.exits >= 1,
            "{}: no recursion trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// P-M3: the native recursion traces are installed and entered.
#[test]
fn pm3_programs_install_and_enter_traces() {
    let mut j = Job::new("pm3_rec", "always").opts("+verify");
    j.limits = "recnat=1".into();
    let res = compare(vec![j]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 2 && o.stats.entered >= 1,
            "{}: no recursion trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// The vararg programs install traces that run, and the counters say what the recorder did.
#[test]
fn pe_programs_install_and_enter_traces() {
    let must = ["vararg_fwd", "vararg_loop", "vararg_types"];
    let mut jobs = Vec::new();
    for p in must {
        jobs.push(Job::new(p, "always").opts("+verify"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
            "{}: no trace ran: {}",
            j.prog,
            o.stats.line()
        );
        eprintln!("{:<22} {}", j.prog, o.stats.line());
    }
}

/// GC stress 1 / 2 over the P-E programs (the exits materialise vararg frames above stale registers; the collector scans them).
#[test]
fn pe_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in PE {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p, "on2")
                    .opts("+verify")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
        for mode in ["gen", "inc"] {
            jobs.push(
                Job::new(p, "always")
                    .opts("+verify")
                    .env("TR_CORE_GCMODE", mode),
            );
        }
    }
    compare(jobs);
}

/// Mutants of the pipeline (feature `tr_mutants`, `RUSTYNAIL_JIT_MUT`): each must make some job of the family diverge
/// from the interpreter or crash its child. The lower-level suites catch them as well; this checks that the live
/// programs do.
#[cfg(feature = "tr_mutants")]
#[test]
fn mutants_are_caught_by_live_programs() {
    // Not here: `poll_skip` (only an asynchronous INTERRUPT reaches a running trace), `tbar_skip`, `popped_frame_drop`,
    // `exit_top_off_by_one`, `exit_th_top_not_written`, `exit_kcache_wrong` (state no program reads: tr_asm / tr_exit / tr_record replay
    // tests compare the full state), `upval_range_skip`, `entry_no_extent`, `fcmp_negated` (differs from the correct compare
    // only for NaN operands; until F2b a live program caught it as a side effect, an unrestored temporary register on the
    // exit path of a compare with a constant operand; `tr_asm::emu_peeled_matches_interp` is its detector) and the
    // recorder-level mutants.
    const MUTANTS: &[&str] = &[
        "rolled_no_writeback",
        "snap_drop_last",
        "snap_prune_dead",
        "entry_no_typecheck",
        "callee_guard_skip",
        "entry_no_ci_spare",
        // F1: the hash part (the programs `field_*` of tests/tr_core/progs.rs)
        "hrefk_key_skip",
        "hload_typecheck_skip",
        "hstore_oldcheck_skip",
        "hstore_oldcheck_dropped",
        "meta_flag_skip",
        "hash_nil_meta_skip",
        "alias_hashtab_as_must",
        // P-A: the `__index` chain guards (the programs `oop_chain_*`)
        // (`hmiss_lsize_skip` is the recorder test's: a rehashed instance still fails the link checks)
        "hmiss_skip",
        "hmiss_next_skip",
        "hmiss_key_skip",
        "index_mt_null_skip",
        "index_tab_typecheck_skip",
        "index_nilnode_skip",
        "chain_end_skip",
        // budget accounting does not change a program's output: only the 9.5 identity sees these
        #[cfg(feature = "t15_oracle")]
        "budget_no_store",
        #[cfg(feature = "t15_oracle")]
        "refund_off_by_one",
        #[cfg(feature = "t15_oracle")]
        "land_pre_mid_swap",
        #[cfg(feature = "t15_oracle")]
        "exit_refund_sign_flipped",
    ];
    let family = |m: &str| -> Vec<Job> {
        // the rolled layout (the writeback) is only produced with `-loop` since P7 peels by default
        let lo = if m == "rolled_no_writeback" {
            "-loop"
        } else {
            ""
        };
        let mut jobs = Vec::new();
        for p in selected(false) {
            jobs.push(
                Job::new(p.name, "always")
                    .opts(lo)
                    .env("RUSTYNAIL_JIT_MUT", m),
            );
            jobs.push(
                Job::new(p.name, "always")
                    .opts(&format!("+exitstorm=3 {lo}"))
                    .env("RUSTYNAIL_JIT_MUT", m),
            );
            jobs.push(
                Job::new(p.name, "always")
                    .budget(17)
                    .opts(lo)
                    .env("RUSTYNAIL_JIT_MUT", m),
            );
        }
        #[cfg(feature = "t15_oracle")]
        for p in ["int_sum", "inline_exit", "float_forloop", "tc_acc"] {
            for b in [None, Some(3), Some(17)] {
                let mut j = Job::new(p, "always").opts(lo).env("RUSTYNAIL_JIT_MUT", m);
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
        }
        for p in [
            "collectable_stores",
            "gc_in_loop",
            "alias_arrays",
            "barrier_stress",
            "alloc_ctor_hash",
            "alloc_escape",
        ] {
            jobs.push(
                Job::new(p, "always")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_JIT_MUT", m),
            );
        }
        jobs
    };
    // the unmutated family passes
    compare(family(""));
    let mut missed = Vec::new();
    let only = std::env::var("TR_CORE_MUTANT").ok();
    for m in MUTANTS {
        if only.as_ref().is_some_and(|o| !m.contains(o.as_str())) {
            continue;
        }
        let r = compare_try(family(m));
        eprintln!(
            "mutant {m}: {}",
            if r.is_ok() { "NOT CAUGHT" } else { "caught" }
        );
        if r.is_ok() {
            missed.push(*m);
        }
    }
    assert!(
        missed.is_empty(),
        "mutants not caught by the live programs: {missed:?}"
    );
}

// ---- P8a: the mutant probe of `tr_mutants.rs` ------------------------------------------------------------------------------

/// A small family of live jobs that passes without a mutant and diverges from the interpreter (or crashes / hangs its
/// child) under the pipeline mutants that corrupt program-visible state (`snap_drop_last`, `snap_prune_dead`,
/// `rolled_no_writeback`, ...; the same families as `mutants_are_caught_by_live_programs`, but driven by the environment:
/// the children inherit `RUSTYNAIL_JIT_MUT`, so `tr_mutants.rs` can run this one test with and without a mutant).
#[test]
fn mutant_probe_live_programs() {
    let mut jobs = Vec::new();
    for p in selected(false) {
        jobs.push(Job::new(p.name, "always"));
        jobs.push(Job::new(p.name, "always").opts("+exitstorm=3"));
        jobs.push(Job::new(p.name, "always").budget(17));
    }
    for p in [
        "collectable_stores",
        "gc_in_loop",
        "alias_arrays",
        "barrier_stress",
        "alloc_ctor_hash",
        "alloc_escape",
    ] {
        jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", "2"));
    }
    compare(jobs);
}

// ---- S0: allocation sinking (docs/JIT_TRACE.md 6.8) ----------------------------------------------------------------------------------

/// The sinking programs install traces that sink tables (`sink: tables N`), run natively, and give the interpreter's output in
/// every mode: exits that materialise tables (`+exitstorm`, `+verify`, `+randra`), tiny budgets, the rolled layout, the `+wb`
/// writeback, and with sinking off.
#[test]
fn sink_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in SINK {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+wb +exitstorm=5 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("+wb"));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
        jobs.push(Job::new(p, "always").opts("-sink"));
    }
    let res = compare(jobs);
    for p in [
        "sink_basic",
        "sink_exit_mid",
        "sink_alias_frames",
        "sink_hooks_getlocal",
    ] {
        let sunk = res
            .iter()
            .filter(|(j, _)| j.prog == p && j.trace == "always" && j.opts == "+verify")
            .map(|(_, o)| o.stats.sunk)
            .max()
            .unwrap_or(0);
        assert!(sunk >= 1, "{p}: no table was sunk");
    }
    for (j, o) in &res {
        if j.opts == "-sink" {
            assert_eq!(o.stats.sunk, 0, "{}: -sink sank a table", j.prog);
        }
    }
}

/// J1-2: the inline materialising exit stub (`sinkstub`): the sinking programs give the interpreter's output with the stub on, in every mode
/// of `sink_programs_run_on_traces` (exit storms make nearly every exit one that materialises), under GC stress, with the emulated trace
/// (`+emurun`: the stub's `CallH` runs the real helper) and with tiny budgets; the stub is really used (`stub-calls` in the sink line), and
/// is not with the knob off. Detector of the mutants `sinkstub_never`, `sinkstub_save_skip`.
#[test]
fn j1_sinkstub_programs() {
    const ON: &str = "sinkmt=1,sinkside=1,sinkstub=1";
    let mut jobs = Vec::new();
    for p in SINK {
        for trace in ["always", "on2"] {
            for opts in [
                "+verify",
                "+exitstorm=3",
                "+wb +exitstorm=5 +verify",
                "+exitstorm=7 +verify +randra=3",
                "+exitstorm=3 +emurun",
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                j.limits = ON.into();
                jobs.push(j);
            }
            let mut j = Job::new(p, trace).budget(17);
            j.limits = ON.into();
            jobs.push(j);
        }
        if *p != "sink_count"
            && *p != "sink_count_stopped"
            && *p != "sink_pacing"
            && *p != "sinkroot_pre"
        {
            for stress in ["1", "2"] {
                let mut j = Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress);
                j.limits = ON.into();
                jobs.push(j);
            }
        }
        let mut j = Job::new(p, "always").opts("+exitstorm=3");
        j.limits = "sinkmt=1,sinkside=1,sinkstub=0".into();
        jobs.push(j);
        // no side traces: the frameful exits stay on their stubs
        for opts in ["+verify", "+exitstorm=7 +verify +randra=3"] {
            let mut j = Job::new(p, "always").opts(opts);
            j.limits = format!("{ON},maxside=0");
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    let mut used = 0;
    for (j, o) in &res {
        if j.limits.ends_with("sinkstub=0") {
            assert_eq!(
                o.stats.stub_calls, 0,
                "{}: the stub was used with sinkstub=0",
                j.prog
            );
        } else if j.opts == "+verify" && j.trace == "always" && !j.limits.contains("maxside") {
            used += o.stats.stub_calls;
        }
    }
    assert!(
        used >= 50,
        "the inline materialising stub was hardly used ({used} calls)"
    );
    let framed: u64 = res
        .iter()
        .filter(|(j, _)| {
            j.prog == "j1_sinkf" && j.opts == "+verify" && j.limits.contains("maxside")
        })
        .map(|(_, o)| o.stats.stub_frames)
        .sum();
    assert!(
        framed >= 1,
        "the frameful inline materialising stub was never used"
    );
}

/// J1-2: a host allocation that fails inside the helper of the inline stub raises the Lua memory error at the exit's pc, as the handler does
/// (`sink_materialise_alloc_failure_raises_memory_error`): exactly one call fails, the others are exact, whichever allocation fails.
/// Detector of the mutant `sinkstub_err_ignored`.
#[test]
fn j1_sinkstub_alloc_failure_raises_memory_error() {
    let mut jobs = Vec::new();
    for n in 1..=4 {
        for (trace, opts) in [("always", ""), ("on2", ""), ("always", "+emurun")] {
            let mut j = Job::new("sink_mem_error", trace)
                .opts(opts)
                .env("TR_CORE_MATFAULT", &n.to_string());
            j.limits = "sinkstub=1".into();
            jobs.push(j);
        }
    }
    let mut j = Job::new("sink_mem_error", "always")
        .env("TR_CORE_MATFAULT", "3")
        .env("RUSTYNAIL_GC_STRESS", "1");
    j.limits = "sinkstub=1,retf=2".into();
    jobs.push(j);
    // the frameful stub (no side traces: the exits stay on their stubs)
    for n in 1..=4 {
        for (trace, opts) in [("always", ""), ("on2", ""), ("always", "+emurun")] {
            let mut j = Job::new("j1_sinkf_mem_error", trace)
                .opts(opts)
                .env("TR_CORE_MATFAULT", &n.to_string());
            j.limits = "sinkstub=1,maxside=0".into();
            jobs.push(j);
        }
    }
    for (j, o) in run_all(jobs) {
        let tag = format!("{} {} {:?}", j.trace, j.opts, j.env);
        if verbose() {
            eprintln!("{:<20} {tag:<60} {}", j.prog, o.stats.line());
        }
        assert!(o.ok, "{tag}: child failed\n{}\n{}", o.result, o.stderr);
        assert!(
            o.result.contains("true\t1\t") && o.result.contains("not enough memory"),
            "{tag}: {}",
            o.result
        );
        assert!(o.result.contains("STATUS true nil"), "{tag}: {}", o.result);
    }
    for p in ["sink_mem_error", "j1_sinkf_mem_error"] {
        let mut j = Job::new(p, "always");
        j.limits = "sinkstub=1,maxside=0".into();
        let o = run_job(&j);
        assert!(o.result.contains("true\t0\tnil"), "{p}: {}", o.result);
    }
}

/// SINK-F: an exit inside inlined frames that names sunk tables gets a side trace (the frame stub materialises, then continues through the exit
/// cell with the frames virtual): `sinkf_side` / `sinkf_side_mem_error` give the interpreter's output in every mode (exit storms, budgets, GC stress,
/// the emulated trace, the stack-move stress, the stub on and off, `maxside=0` as the control) and side traces really are installed. Detector of
/// the mutant `sunkframe_cell_nomat`.
#[test]
fn sinkf_side_programs() {
    const ON: &str = "sinkmt=1,sinkside=1,sinkstub=1";
    let mut jobs = Vec::new();
    for p in [
        "sinkf_side",
        "sinkf_side_mem_error",
        "j1_sinkf",
        "sink_alias_frames",
    ] {
        for trace in ["always", "on2", "on16"] {
            for opts in [
                "",
                "+verify",
                "+exitstorm=3",
                "+wb +exitstorm=5 +verify",
                "+exitstorm=7 +verify +randra=3",
                "+exitstorm=3 +emurun",
                "-loop -fwd -cse -sink",
            ] {
                let mut j = Job::new(p, trace).opts(opts);
                j.limits = ON.into();
                jobs.push(j);
            }
            let mut j = Job::new(p, trace).budget(17);
            j.limits = ON.into();
            jobs.push(j);
            let mut j = Job::new(p, trace);
            j.limits = "sinkmt=1,sinkside=1,sinkstub=0".into();
            jobs.push(j);
            let mut j = Job::new(p, trace);
            j.limits = format!("{ON},maxside=0");
            jobs.push(j);
        }
        for stress in ["1", "2"] {
            for gm in ["gen", "inc"] {
                let mut j = Job::new(p, "always")
                    .env("RUSTYNAIL_GC_STRESS", stress)
                    .env("TR_CORE_GCMODE", gm);
                j.limits = ON.into();
                jobs.push(j);
            }
        }
        let mut j = Job::new(p, "always").env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = ON.into();
        jobs.push(j);
        // the sink policies on top (a demotion flushes the family from inside the stub's helper: the stub must rebuild its frames, not enter a stale cell)
        for pol in ["sinkroot=1", "sinkroot=2"] {
            for trace in ["always", "on2"] {
                let mut j = Job::new(p, trace);
                j.limits = format!("{ON},{pol}");
                jobs.push(j);
                let mut j = Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2");
                j.limits = format!("{ON},{pol}");
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    let (mut side, mut framed) = (0, 0);
    for (j, o) in &res {
        if j.prog == "sinkf_side" && j.trace == "always" && j.opts == "+verify" && j.limits == ON {
            side += o.stats.side_installed;
            framed += o.stats.stub_frames;
        }
    }
    assert!(side >= 1, "sinkf_side: no side trace was installed");
    assert!(
        framed >= 1,
        "sinkf_side: the frameful materialising stub was never used"
    );
}

/// SINK-F: a host allocation failure in the helper of the frame stub of an exit that HAS a side trace raises the memory error at the exit's pc
/// and leaves the frames rebuilt (`sinkf_side_mem_error`: exactly one call fails, the others are exact).
#[test]
fn sinkf_side_alloc_failure_raises_memory_error() {
    let mut jobs = Vec::new();
    for n in 1..=6 {
        for (trace, opts) in [
            ("always", ""),
            ("on2", ""),
            ("always", "+emurun"),
            ("on16", ""),
        ] {
            let mut j = Job::new("sinkf_side_mem_error", trace)
                .opts(opts)
                .env("TR_CORE_MATFAULT", &n.to_string());
            j.limits = "sinkmt=1,sinkside=1,sinkstub=1".into();
            jobs.push(j);
        }
    }
    for (j, o) in run_all(jobs) {
        let tag = format!("{} {} {:?}", j.trace, j.opts, j.env);
        assert!(o.ok, "{tag}: child failed\n{}\n{}", o.result, o.stderr);
        assert!(
            o.result.contains("true\t1\t") && o.result.contains("not enough memory"),
            "{tag}: {}",
            o.result
        );
    }
    let mut j = Job::new("sinkf_side_mem_error", "always");
    j.limits = "sinkmt=1,sinkside=1,sinkstub=1".into();
    let o = run_job(&j);
    assert!(o.result.contains("true\t0\tnil"), "no fault: {}", o.result);
}

/// SINK-R: the ineffective-root rule is sink-aware (`sinkroot`): the output is the interpreter's with the rule off, global and head-scoped, in
/// every mode, and the root that the rule used to flush and blacklist is re-recorded without sinking instead (fewer blacklisted heads, fewer sunk
/// tables). Detector of the mutants `sinkroot_never`, `sink_demote_ignored`.
#[test]
fn sinkroot_programs() {
    // (the shipped process defaults of the recorder that make the producer / consumer closures traceable; `second=0`: the shipped second chance
    // of a blacklisted head would also keep the head of the `sinkroot=0` run out of the blacklist, which is what this test counts)
    const BASE: &str = "recnat=1,recany=1,loopany=1,second=0";
    let mut jobs = Vec::new();
    for p in ["sinkroot_pre", "sinkf_side"] {
        for sr in ["sinkroot=0", "sinkroot=1", "sinkroot=2"] {
            for trace in ["always", "on2"] {
                for opts in ["", "+verify", "+exitstorm=7 +verify +randra=3"] {
                    let mut j = Job::new(p, trace).opts(opts);
                    j.limits = format!("{BASE},sinkmt=1,sinkside=1,sinkstub=1,{sr}");
                    jobs.push(j);
                }
            }
            let mut j = Job::new(p, "always");
            j.limits = format!("{BASE},{sr}");
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    let pick = |sr: &str| {
        let l = format!("{BASE},{sr}");
        res.iter()
            .find(|(j, _)| {
                j.prog == "sinkroot_pre"
                    && j.trace == "always"
                    && j.opts.is_empty()
                    && j.limits == l
            })
            .unwrap()
            .1
            .stats
            .clone()
    };
    let off = pick("sinkroot=0");
    assert!(
        off.ineffective >= 1,
        "the program does not reach the ineffective-root rule: {}",
        off.line()
    );
    for sr in ["sinkroot=1", "sinkroot=2"] {
        let on = pick(sr);
        assert!(
            off.blacklisted > on.blacklisted,
            "{sr}: the rule did not keep a head from the blacklist: {} vs {}",
            on.line(),
            off.line()
        );
        // the demoted shape is not sunk by the recording that follows (`sink::is_demoted`, mutant `sink_demote_ignored`)
        assert!(
            off.sunk > on.sunk,
            "{sr}: the re-recorded root sinks the demoted shape again: {} vs {}",
            on.line(),
            off.line()
        );
    }
}

/// GC stress 1 and 2 (every allocation / safepoint steps the collector) and the collector verifier on the small programs.
#[test]
fn sink_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in SINK {
        if *p == "sink_count"
            || *p == "sink_count_stopped"
            || *p == "sink_pacing"
            || *p == "sinkroot_pre"
        {
            continue; // not small
        }
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            jobs.push(
                Job::new(p, "on2")
                    .opts("+verify")
                    .env("RUSTYNAIL_GC_STRESS", stress),
            );
        }
        jobs.push(
            Job::new(p, "always")
                .env("RUSTYNAIL_GC_STRESS", "2")
                .env("RUSTYNAIL_GC_VERIFY", "1"),
        );
    }
    compare(jobs);
}

/// TR29.3: a host allocation that fails while an exit materialises sunk tables raises the Lua memory error at the exit's landing
/// pc (no panic, the frame is consistent): exactly one call of the program fails with "not enough memory", the others are exact,
/// whichever allocation (the table, its resize, the first or the second field) fails.
#[test]
fn sink_materialise_alloc_failure_raises_memory_error() {
    let mut jobs = Vec::new();
    for n in 1..=4 {
        for trace in ["always", "on2"] {
            jobs.push(Job::new("sink_mem_error", trace).env("TR_CORE_MATFAULT", &n.to_string()));
        }
    }
    // (`retf=2`: the shape of the allocations with GC stress 1 is the one of the pre-RETF default: the third materialisation does not exist with side RETF recording)
    jobs.push(
        Job::new("sink_mem_error", "always")
            .env("TR_CORE_MATFAULT", "3")
            .env("RUSTYNAIL_GC_STRESS", "1")
            .limits_s("retf=2"),
    );
    // F6: `+emurun` (the exit handler runs inside the emulated trace) returns `JitExit::Error` to the interpreter like the machine code
    for n in 1..=4 {
        jobs.push(
            Job::new("sink_mem_error", "always")
                .opts("+emurun")
                .env("TR_CORE_MATFAULT", &n.to_string()),
        );
    }
    // K3b: `sinkstub=0` routes the materialisation through the exit handler (`exit.rs`, mutant `sink_mem_error_dropped`); the shipped inline stub calls `jit_tr_mat`
    for n in 1..=4 {
        jobs.push(
            Job::new("sink_mem_error", "always")
                .env("TR_CORE_MATFAULT", &n.to_string())
                .limits_s("sinkstub=0"),
        );
    }
    for (j, o) in run_all(jobs) {
        let tag = format!("{} {:?}", j.trace, j.env);
        assert!(o.ok, "{tag}: child failed\n{}\n{}", o.result, o.stderr);
        assert!(
            o.result.contains("true\t1\t") && o.result.contains("not enough memory"),
            "{tag}: {}",
            o.result
        );
        assert!(o.result.contains("STATUS true nil"), "{tag}: {}", o.result);
    }
    // and no failure without the injection
    let o = run_job(&Job::new("sink_mem_error", "always"));
    assert!(o.result.contains("true\t0\tnil"), "{}", o.result);
}

/// With the collector stopped the byte count after a sinking loop equals the interpreter's (virtual bytes are charged like
/// real allocations, and a materialised table moves from virtual to real), generational and incremental.
#[test]
fn sink_byte_count_matches_the_interpreter() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        jobs.push(Job::new("sink_count_stopped", trace));
        jobs.push(Job::new("sink_count_stopped", trace).opts("+exitstorm=3"));
        jobs.push(Job::new("sink_count_stopped", trace).opts("+wb"));
    }
    compare(jobs);
}

/// The P-B programs.
const PB: &[&str] = &["lib_str", "lib_tab", "str_concat", "str_temporaries"];

/// The library-call intrinsics install and run traces, in `always` and `on2`, with the lockstep, with every guard failing now and then
/// and with small budgets: the same results as the interpreter.
#[test]
fn pb_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in PB {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=3"));
            jobs.push(Job::new(p, trace).budget(17));
        }
        jobs.push(Job::new(p, "always").opts("-loop -fwd"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// GC stress 1 / 2 over the P-B programs, in both collector modes.
#[test]
fn pb_programs_under_gc_stress() {
    let mut jobs = Vec::new();
    for p in PB {
        for stress in ["1", "2"] {
            jobs.push(Job::new(p, "always").env("RUSTYNAIL_GC_STRESS", stress));
            for mode in ["gen", "inc"] {
                jobs.push(
                    Job::new(p, "always")
                        .env("RUSTYNAIL_GC_STRESS", stress)
                        .env("TR_CORE_GCMODE", mode),
                );
            }
        }
    }
    compare(jobs);
}

/// The exit-storm rule: allocation exits that follow each other within a few instructions, 64 times in a row, give the loop back;
/// exits far apart (or the budget word going up: a refill) restart the count.
#[test]
fn pb_alloc_storm_gives_the_loop_back() {
    use rn_jit::tr::cache::{storm_step, STORM_INSNS, STORM_STREAK};
    let mut e = (1_000_000i32, 0u32);
    let mut b = 1_000_000i32;
    let mut fired = 0;
    for _ in 0..STORM_STREAK * 2 {
        b -= 13; // one iteration of 13 instructions between two exits
        if storm_step(&mut e, b) {
            fired += 1;
        }
    }
    assert_eq!(fired, 2, "a storm fires every {STORM_STREAK} exits");
    // sparse exits never fire
    let mut e = (1_000_000i32, 0u32);
    for k in 1..=STORM_STREAK * 3 {
        assert!(!storm_step(
            &mut e,
            1_000_000 - k as i32 * (STORM_INSNS as i32 + 1)
        ));
    }
    // a refill of the budget (the word goes up) restarts the streak
    let mut e = (1_000_000i32, 0u32);
    let mut b = 1_000_000i32;
    for k in 0..STORM_STREAK * 2 {
        b -= 13;
        if k % 10 == 9 {
            b += 5000;
        }
        assert!(!storm_step(&mut e, b));
    }
}

// ---- N5: string loads that do not specialise on the short / long variant, and the ping-pong rule -----------------------------------

/// The programs that read strings of both variants (and callees / numbers of mixed variants) from tables run natively with the
/// interpreter's output in every mode (exit storms, `+verify`, random register allocation, GC stress, small budgets, the pass off).
/// Detector of the mutant `relaxstr_any_use` (the "some string" guard is applied where a consumer needs the variant: `#s` of a long
/// string reads the short string's length byte).
#[test]
fn string_variant_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in ["n5_str_variants", "n5_callee_variants"] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-relaxstr"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n5_str_variants" && j.trace == "always" && j.opts == "+verify" {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// The cliff shape of `bench/lj/cliff_mixed_strings.lua`: no root is flushed as ineffective (the strings of the `unused` loop are
/// never observed; the `len` loop ping-pongs with its side traces while completing iterations between the hops).
#[test]
fn mixed_string_loops_are_not_flushed_as_ineffective() {
    let res = compare(vec![
        Job::new("n5_cliff_shape", "always"),
        Job::new("n5_cliff_shape", "on"),
        Job::new("n5_cliff_shape", "always").opts("-relaxstr"),
        // K3a: the strict exit bound below is the one of the pass without `hdrlib` / `typesides`; with both shipped the `len` loop takes a side-trace
        // hop (counted as an exit) on its 2 of 8 variant loads and the loops complete in the same number of iterations
        Job::new("n5_cliff_shape", "always").limits_s("hdrlib=0,typesides=0"),
        Job::new("n5_cliff_shape", "on").limits_s("hdrlib=0,typesides=0"),
    ]);
    for (j, o) in &res {
        assert_eq!(
            o.stats.ineffective,
            0,
            "{} {} {}: flushed as ineffective: {}",
            j.prog,
            j.trace,
            j.opts,
            o.stats.line()
        );
        assert!(
            o.stats.installed >= 1,
            "{} {}: {}",
            j.prog,
            j.trace,
            o.stats.line()
        );
        if j.opts.is_empty() {
            // the string of the `unused` and `vararg` loops is only a snapshot value: no trace exit at all (without the pass these
            // loops leave their trace on every other iteration: ~25000 exits)
            let bound = if j.limits.is_empty() { 20000 } else { 1000 };
            assert!(
                o.stats.exits < bound,
                "{} {} [{}]: {}",
                j.prog,
                j.trace,
                j.limits,
                o.stats.line()
            );
        }
    }
}

// ---- N1: upvalues that point into the root frame are recorded as register accesses -------------------------------------------

/// Closures that capture locals of the loop's own frame (read-only, read-write, shared, nested, closed by an exit, vararg frames,
/// coroutines, pcall) record and run natively (the old `UpvalAlias` abort is gone) with the interpreter's output in every mode.
/// Detector of the mutants `uvslot_guard_skip` (another closure of the proto has its upvalue elsewhere), `uvslot_off_by_one`
/// (the guard names the wrong slot), `uvslot_as_cell` (the access stays a memory access behind the SSA slot values) and
/// `uvslot_store_skip` (a store through the upvalue is lost).
#[test]
fn upvalue_slot_alias_programs() {
    let mut jobs = Vec::new();
    for p in ["n1_alias_basic", "n1_alias_frames"] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n1_alias_basic" && j.trace == "always" && j.opts == "+verify" {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

// ---- N6: guard-free hash lookup for dynamic short-string keys (HREFV) -------------------------------------------------------------

// ---- N7: ipairs(t) / pairs(t) calls, TFORPREP and the end of a traversal in traces ---------------------------------------

/// The N7 programs (`ipairs(t)` / `pairs(t)` inside traced loops; metatables, shadowing, non-tables, results; coroutines, closures,
/// closing values, deep nests): the same output as the interpreter in every mode (verify, exit storms, register eviction, emulator, GC
/// stress 2, without the loop optimiser / forwarding / CSE / sinking, small budgets, recording switched off). Detector of the mutants
/// `gfor_pairs_meta_skip` (`__pairs` appearing in the middle of the loop), `gfor_wrong_iterator`, `gfor_result_count`,
/// `gfor_ipairs_start_one`, `gfor_end_meta_skip` (`__index` appearing), `gfor_end_guard_skip` (an element that is not nil), `gfor_end_always`
/// and `tforprep_no_swap`.
#[test]
fn gfor_programs() {
    let mut jobs = Vec::new();
    for p in [
        "n7_gfor_nest",
        "n7_gfor_meta",
        "n7_gfor_misc",
        "n7_nest_hot",
    ] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n7_gfor_nest"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// N3 stage 1: metamethods that are Lua closures (`__add` ... `__concat`, `__unm`, `__len`) are called from traces as inlined frames.
/// The programs run with the interpreter's output in every mode (verify, exit storms, register eviction, emulator, GC stress 2, without
/// the loop optimiser / forwarding, small budgets). Detector of the mutants `mm_flag_skip`, `mm_ctx_off`, `mm_ncalls_skip`
/// (exits inside the metamethod body: errors, yields), `mm_follower_free`, `mm_result_skip`, `mm_result_slot_off`, `mm_mt_null_skip`,
/// `mm_tm_typecheck_skip`, `mm_callee_guard_skip`, `mm_num_mt_skip`, `mm_absent_skip` and `mm_ccalls_skip`.
#[test]
fn mm_programs() {
    let mut jobs = Vec::new();
    for p in [
        "n3_mm_arith",
        "n3_mm_swap",
        "n3_mm_nummt_late",
        "n3_mm_err",
        "n3_mm_exit",
        "n3_mm_depth",
        "n3_mm_cmp",
        "n3_mm_idx",
        "n3_mm_idx2",
        "nn_tail_call",
        "nn_idx_chain",
    ] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n3_mm_arith"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
        if (j.prog == "nn_tail_call" || j.prog == "nn_idx_chain")
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            // BUNDLE-N: a `TAILCALL` of a table with `__call` / into a vararg callee, an `__index` function behind a table chain and
            // `rawget(t, "str")` record: traces install, and no recording ends `Diverge` / `Frame` (a frame the interpreter does not run in place)
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "{}: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                !o.stats.aborts.contains_key("Frame") && !o.stats.aborts.contains_key("Diverge"),
                "{}: {}",
                j.prog,
                o.stats.line()
            );
        }
        if j.prog == "n3_mm_cmp"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            // (a comparison whose recorded successor is not the interpreter's -- the wrong condition -- aborts the recording `Frame`)
            assert!(o.stats.installed >= 6, "{}: {}", j.prog, o.stats.line());
            assert!(
                !o.stats.aborts.contains_key("Frame") && !o.stats.aborts.contains_key("Diverge"),
                "{}: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// BUNDLE-S: `loopany` with its guard (`loopanymax`): the program `s_loopany` (a callee `while true` of pseudo-random trip count, one of constant
/// trip count, a `repeat`) agrees with the interpreter in every mode; the unguarded unrolling needs more side traces than the guarded one (the
/// guard flushes the family after `loopanymax` side attempts inside callee frames and blocks the loops), and `loopany=0` records no unrolled loop.
/// Mutant `loopany_guard_never`.
#[test]
fn s_loopany_guard_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "loopany=0",
            "loopany=1",
            "loopany=1,loopanymax=2",
            "loopany=1,loopanymax=100000",
        ] {
            jobs.push(Job::new("s_loopany", trace).opts("+verify").limits_s(lim));
        }
        jobs.push(
            Job::new("s_loopany", trace)
                .opts("+exitstorm=3 +verify")
                .limits_s("loopany=1"),
        );
        jobs.push(
            Job::new("s_loopany", trace)
                .budget(17)
                .limits_s("loopany=1"),
        );
        jobs.push(
            Job::new("s_loopany", trace)
                .env("RUSTYNAIL_GC_STRESS", "2")
                .limits_s("loopany=1"),
        );
        jobs.push(
            Job::new("s_loopany", trace)
                .opts("+emurun +verify")
                .limits_s("loopany=1"),
        );
    }
    jobs.push(Job::new("s_loopany", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "s_loopany"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, guard, wide) = (
            find(trace, "loopany=0"),
            find(trace, "loopany=1"),
            find(trace, "loopany=1,loopanymax=100000"),
        );
        eprintln!(
            "s_loopany [{trace}] off: {} | guard: {} | unguarded: {}",
            off.line(),
            guard.line(),
            wide.line()
        );
        assert!(
            wide.side_installed > guard.side_installed,
            "[{trace}] the guard stops the side traces of the unrolled loop: {} vs {}",
            guard.line(),
            wide.line()
        );
    }
}

/// BUNDLE-P: `laprune`. `s_laprune` is the shape of awfy/cd (a root that unrolls no callee loop, side traces that unroll a data-dependent `while`): the guard
/// blocks the loop and flushes only the unrolling side traces, the root survives (no `Ineffective` flush, no family flush), and one of the pruned side traces is
/// the trace whose own exit raised the guard (flushed under its exit handler). `s_loopany` is the other shape (the root unrolled, the `heapsort` case): the whole
/// family is flushed, nothing pruned. Same output as the interpreter in every mode (also with the guard tight, GC stress, emulator). Mutants
/// `laprune_flushes_root`, `laprune_never_flushes_sides`.
#[test]
fn p_laprune_programs() {
    let mut jobs = Vec::new();
    for p in ["s_laprune", "s_loopany"] {
        for trace in ["always", "on2"] {
            for lim in [
                "loopany=0,nilstore=1,siderooms=1,retf=2",
                "laprune=0,loopany=1,nilstore=1,siderooms=1,retf=2",
                "laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2",
                "laprune=1,loopany=1,nilstore=1,siderooms=1,loopanymax=1,retf=2",
                "laprune=1,loopany=1,nilstore=1,siderooms=1,loopanymax=2,retf=2",
                "laprune=1,loopany=1,nilstore=1,siderooms=1,loopanymax=3,retf=2",
                "laprune=1,loopany=1,nilstore=1,siderooms=1,loopanymax=100000,retf=2",
            ] {
                jobs.push(Job::new(p, trace).opts("+verify").limits_s(lim));
            }
            jobs.push(
                Job::new(p, trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s("laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2"),
            );
            jobs.push(
                Job::new(p, trace)
                    .budget(17)
                    .limits_s("laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2"),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2"),
            );
            jobs.push(
                Job::new(p, trace)
                    .opts("+emurun +verify")
                    .limits_s("laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2"),
            );
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    let find = |p: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == p
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on) = (
            find(
                "s_laprune",
                trace,
                "laprune=0,loopany=1,nilstore=1,siderooms=1,retf=2",
            ),
            find(
                "s_laprune",
                trace,
                "laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2",
            ),
        );
        eprintln!(
            "s_laprune [{trace}] laprune=0: {} guard {}/{}/{}/{} | laprune=1: {} guard {}/{}/{}/{}",
            off.line(),
            off.la_fired,
            off.la_family,
            off.la_pruned,
            off.la_self,
            on.line(),
            on.la_fired,
            on.la_family,
            on.la_pruned,
            on.la_self
        );
        assert!(
            on.la_fired >= 1 && on.la_pruned >= 1,
            "[{trace}] the guard fires and prunes side traces: {}",
            on.line()
        );
        assert_eq!(
            on.la_family,
            0,
            "[{trace}] the root unrolled nothing: no family flush: {}",
            on.line()
        );
        assert_eq!(
            on.ineffective,
            0,
            "[{trace}] the root is not flushed: {}",
            on.line()
        );
        assert!(
            on.la_self >= 1,
            "[{trace}] a pruned side trace raised the guard from its own exit: {}",
            on.line()
        );
        assert!(
            off.la_fired >= 1 && off.la_family >= 1 && off.la_pruned == 0 && off.ineffective >= 1,
            "[{trace}] laprune=0 flushes the family: {}",
            off.line()
        );
        // exits stay bounded: not more than twice those of `loopany=0` (no unrolled loop at all), and about one per entry
        let none = find(
            "s_laprune",
            trace,
            "loopany=0,nilstore=1,siderooms=1,retf=2",
        );
        assert!(
            on.exits <= 2 * none.exits && on.exits <= 3 * on.entered,
            "[{trace}] exits stay bounded: {} (entered {}) vs loopany=0 {}",
            on.exits,
            on.entered,
            none.exits
        );
        // the heapsort shape: the root itself unrolled the callee loop, the whole family goes
        let h = find(
            "s_loopany",
            trace,
            "laprune=1,loopany=1,nilstore=1,siderooms=1,retf=2",
        );
        assert!(
            h.la_fired >= 1 && h.la_family >= 1 && h.la_pruned == 0,
            "[{trace}] s_loopany: {}",
            h.line()
        );
    }
}

/// C36: `cispare`. `c36_ci` runs a leaf with an inlined `math.floor` (tier 1 `t1inline` bit 1, so no real call ever extends the `CallInfo` chain) in fresh
/// coroutines: every entry of the leaf's loop trace fails its spare-`CallInfo` check (TR6b). With `cispare=0` the entry-failure rule flushes the root
/// (`rerecord` >= 1); with `cispare=1` the handler allocates the node, restarts the failure account and the trace is never flushed. Same output as the
/// interpreter in every mode. Mutants `cispare_never`, `cispare_flush_anyway` (the second repairs and flushes anyway: `rerecord` stays >= 1).
#[test]
fn c36_cispare_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in ["t1inline=1023,cispare=0", "t1inline=1023,cispare=1"] {
            jobs.push(Job::new("c36_ci", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("c36_ci", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("c36_ci", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("c36_ci", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("c36_ci", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
        jobs.push(
            Job::new("c36_ci", trace)
                .opts("+verify")
                .limits_s("t1inline=1021,cispare=0"),
        );
    }
    jobs.push(Job::new("c36_ci", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "c36_ci"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on) = (
            find(trace, "t1inline=1023,cispare=0"),
            find(trace, "t1inline=1023,cispare=1"),
        );
        eprintln!(
            "c36_ci [{trace}] cispare=0: {} | cispare=1: {} repaired {}",
            off.line(),
            on.line(),
            on.ci_repaired
        );
        assert_eq!(off.ci_repaired, 0, "[{trace}] knob off repairs nothing");
        assert!(
            on.ci_repaired >= 1,
            "[{trace}] the chain is extended: {}",
            on.line()
        );
        assert!(
            off.rerecord >= 1,
            "[{trace}] without cispare the entry-failure rule flushes the root: {}",
            off.line()
        );
        if cfg!(rustynail_lua54) {
            // 5.4: the repair happens (`ci_repaired` above), but the root of `mul_small` is flushed by the exit-ratio rule anyway after the repair restarted its
            // entry account (`flush Rerec (entries 0, exits 6000)`), in both knob settings: the 5.5 `rerecord == 0` is a 5.5 policy number. Not worse than off.
            assert!(
                on.rerecord <= off.rerecord,
                "[{trace}] cispare must not add re-records: {} vs {}",
                on.rerecord,
                off.rerecord
            );
        } else {
            assert_eq!(
                on.rerecord,
                0,
                "[{trace}] with cispare the trace stays: {}",
                on.line()
            );
        }
    }
}

/// C36: `domtop`. `c36_dom` records its loop root on the rare branch (the first recorded iteration takes it, later only 1 in 8 does): about 94 % of the entries hop
/// through one side trace. With `domtop=0` the root stays on the rare path; with `domtop=75,domn=2000` it is re-recorded exactly once (`dom_flushes` == 1, no
/// penalty: no `rerecord`/`ineffective` counted) and the exits fall (`always`; in `on2` the root is on the dominant path already). Same output as the interpreter in every mode. Mutants `dom_never`, `dom_repeats`.
#[test]
fn c36_dom_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "ineffcover=1,domtop=0",
            "ineffcover=1,domtop=75,domn=2000",
            "ineffcover=1,domtop=75,domn=2000000",
        ] {
            jobs.push(Job::new("c36_dom", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("c36_dom", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("c36_dom", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("c36_dom", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("c36_dom", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
    }
    jobs.push(Job::new("c36_dom", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "c36_dom"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on, big) = (
            find(trace, "ineffcover=1,domtop=0"),
            find(trace, "ineffcover=1,domtop=75,domn=2000"),
            find(trace, "ineffcover=1,domtop=75,domn=2000000"),
        );
        eprintln!(
            "c36_dom [{trace}] domtop=0: {} | domtop=75: {} dom {} | domn huge: {} dom {}",
            off.line(),
            on.line(),
            on.dom_flushes,
            big.line(),
            big.dom_flushes
        );
        assert_eq!(off.dom_flushes, 0, "[{trace}] knob off");
        assert_eq!(big.dom_flushes, 0, "[{trace}] too few entries for the rule");
        if trace != "always" {
            // (the `on2` start gate records another iteration: the root is on the dominant path already; only the output is compared)
            continue;
        }
        assert_eq!(
            on.dom_flushes,
            1,
            "[{trace}] the root is re-recorded exactly once: {}",
            on.line()
        );
        assert_eq!(
            (on.rerecord, on.ineffective),
            (off.rerecord, off.ineffective),
            "[{trace}] no penalty account is touched"
        );
        assert!(
            on.exits * 3 < off.exits,
            "[{trace}] the re-recorded root takes the dominant path: exits {} vs {}",
            on.exits,
            off.exits
        );
    }
}

/// (K3a: the 12-callee chain is re-ordered with `chaindepth=0` and the old cap `maxside=64` (the chain-no-rebase mutant needs room for the repeated re-orders); under the shipped `chaindepth=4` the chain stops growing at 4 links and is never re-ordered, the `wide` job is that shipped-config control.)
/// P3 `chainre`: a megamorphic dispatch whose distribution changes mid-run (`p3_chain`: 12 callees, warm-up in order, then 9..12 hot, then 1..4 hot). With `chainre=0` no
/// chain is touched; with `chainre=95,chainn=1000` chains are re-ordered (`chain_flushes` >= 1, at most the family bound) and the run takes fewer exits through
/// pass-through links. `p3_uniform` (16 callees used uniformly: the first link passes 94 % of its entries) must NOT fire, however often the chain is checked
/// (`chain_checks` >= 2: the check is periodic). Same output as the interpreter in every mode (verify lockstep, exit storm, budget 17, GC stress 2, the emulator,
/// both trace modes). Mutants `chain_never`, `chain_unbounded`, `chain_pass_ignored`, `chain_gain_ignored`, `chain_stub_never`, `chain_stub_no_test`, `chain_no_rebase`
/// (the rule itself: `tr_policy::p3_policy`).
#[test]
fn p3_chain_programs() {
    let mut jobs = Vec::new();
    for prog in ["p3_chain", "p3_uniform"] {
        for trace in ["always", "on2"] {
            let lims: [&str; 3] = if prog == "p3_chain" {
                [
                    "chainre=0,ncallee=1,chaindepth=0,maxside=64",
                    "chainre=95,chainn=1000,ncallee=1,chaindepth=0,maxside=64",
                    "chainre=95,chainn=1000,ncallee=1",
                ]
            } else {
                [
                    "chainre=0,ncallee=1",
                    "chainre=60,chainn=1000,ncallee=1",
                    "chainre=60,chainn=1000,maxside=64,ncallee=1",
                ]
            };
            for lim in lims {
                jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s(lim),
                );
                jobs.push(Job::new(prog, trace).budget(17).limits_s(lim));
                jobs.push(
                    Job::new(prog, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .limits_s(lim),
                );
                jobs.push(Job::new(prog, trace).opts("+emurun +verify").limits_s(lim));
            }
        }
        jobs.push(Job::new(prog, "t1"));
    }
    let res = compare(jobs);
    let find = |prog: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == prog
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on, wide) = (
            find(
                "p3_chain",
                trace,
                "chainre=0,ncallee=1,chaindepth=0,maxside=64",
            ),
            find(
                "p3_chain",
                trace,
                "chainre=95,chainn=1000,ncallee=1,chaindepth=0,maxside=64",
            ),
            find("p3_chain", trace, "chainre=95,chainn=1000,ncallee=1"),
        );
        eprintln!(
            "p3_chain [{trace}] off: {} | on: {} flushes {} checks {} | wide: {} flushes {}",
            off.line(),
            on.line(),
            on.chain_flushes,
            on.chain_checks,
            wide.line(),
            wide.chain_flushes
        );
        assert_eq!(
            (off.chain_flushes, off.chain_checks),
            (0, 0),
            "[{trace}] knob off"
        );
        // (K3a: 95 / 92 checks without the mutant `chain_no_rebase`, 10 with it: the threshold of 2 no longer told them apart)
        assert!(
            on.chain_checks >= 30,
            "[{trace}] the check is periodic: {}",
            on.chain_checks
        );
        assert!(
            on.chain_flushes >= 1 && on.chain_flushes <= 12 + 3,
            "[{trace}] a chain is re-ordered, within the bound: {}",
            on.chain_flushes
        );
        let (u_off, u_on) = (
            find("p3_uniform", trace, "chainre=0,ncallee=1"),
            find("p3_uniform", trace, "chainre=60,chainn=1000,ncallee=1"),
        );
        eprintln!(
            "p3_uniform [{trace}] off: {} | on: {} flushes {} checks {}",
            u_off.line(),
            u_on.line(),
            u_on.chain_flushes,
            u_on.chain_checks
        );
        assert_eq!(
            u_on.chain_flushes,
            0,
            "[{trace}] a uniform chain is never re-ordered: {}",
            u_on.line()
        );
        assert!(
            u_on.chain_checks >= 30,
            "[{trace}] but it is checked more than once: {}",
            u_on.chain_checks
        );
    }
}

/// BUNDLE-G `chaindepth`: a call through a table of 24 closures used uniformly in pseudo-random order (`bg_wide`) and one through 4 (`bg_narrow`), then the table is
/// replaced under the loop. With `chaindepth=0` no chain is cut (`chain_deep` == 0); with `chaindepth=4` the wide chain stops growing (`chain_deep` >= 1, fewer
/// side traces) and the narrow one is untouched (`chain_deep` == 0: the coverage test). Same output as the interpreter in every mode (verify lockstep, exit storm, budget
/// 17, GC stress 2, the emulator, both trace modes). Mutants `chaindepth_ignored`, `chaindepth_cover_ignored` (the rule: `tr_policy::bg_policy`).
#[test]
fn bg_chaindepth_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "chaindepth=0,ncallee=1,chainre=95,chainn=1000",
            "chaindepth=4,ncallee=1,chainre=95,chainn=1000",
        ] {
            for prog in ["bg_wide", "bg_narrow"] {
                jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s(lim),
                );
                jobs.push(Job::new(prog, trace).budget(17).limits_s(lim));
                jobs.push(
                    Job::new(prog, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .limits_s(lim),
                );
                jobs.push(Job::new(prog, trace).opts("+emurun +verify").limits_s(lim));
            }
        }
    }
    jobs.push(Job::new("bg_wide", "t1"));
    jobs.push(Job::new("bg_narrow", "t1"));
    let res = compare(jobs);
    let find = |prog: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == prog
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on) = (
            find(
                "bg_wide",
                trace,
                "chaindepth=0,ncallee=1,chainre=95,chainn=1000",
            ),
            find(
                "bg_wide",
                trace,
                "chaindepth=4,ncallee=1,chainre=95,chainn=1000",
            ),
        );
        eprintln!(
            "bg_wide [{trace}] off: {} | on: {} deep {}",
            off.line(),
            on.line(),
            on.chain_deep
        );
        assert_eq!(off.chain_deep, 0, "[{trace}] knob off");
        assert!(
            on.chain_deep >= 1,
            "[{trace}] the wide chain is cut: {}",
            on.line()
        );
        let (noff, non) = (
            find(
                "bg_narrow",
                trace,
                "chaindepth=0,ncallee=1,chainre=95,chainn=1000",
            ),
            find(
                "bg_narrow",
                trace,
                "chaindepth=4,ncallee=1,chainre=95,chainn=1000",
            ),
        );
        eprintln!(
            "bg_narrow [{trace}] off: {} | on: {} deep {}",
            noff.line(),
            non.line(),
            non.chain_deep
        );
        assert_eq!(
            non.chain_deep,
            0,
            "[{trace}] a narrow dispatch is covered by its first links: {}",
            non.line()
        );
    }
}

/// P3 `ncallee`: a call through a table of 4 closures in a random order (`p3_fdisp`). With `ncallee=0` the callee guard sits in the entry segment, fails 3 entries in 4 and
/// the loop ends up flushed / blacklisted; with `ncallee=1` the first entry-failure flush sets the head's flag (`ncallee_heads` == 1), the re-recorded root keeps
/// its callee guard in the body and the loop is not blacklisted (`blacklisted` smaller). Same output in every mode. Mutants `ncallee_never`, `ncallee_without_entry_guard`
/// (rule: `tr_policy::p3_policy`).
#[test]
fn p3_fdisp_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in ["ncallee=0", "ncallee=1"] {
            jobs.push(Job::new("p3_fdisp", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("p3_fdisp", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("p3_fdisp", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("p3_fdisp", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("p3_fdisp", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
    }
    jobs.push(Job::new("p3_fdisp", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "p3_fdisp"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on) = (find(trace, "ncallee=0"), find(trace, "ncallee=1"));
        eprintln!(
            "p3_fdisp [{trace}] ncallee=0: {} | ncallee=1: {} heads {}",
            off.line(),
            on.line(),
            on.ncallee_heads
        );
        assert_eq!(off.ncallee_heads, 0, "[{trace}] knob off");
        if cfg!(rustynail_lua54) {
            // 5.4: the callee guard is not in the entry segment of this loop's trace (every exit is an ordinary body exit with a side trace, `entry false` in the
            // exit log), so no entry failure ever happens and the P3 rule has nothing to flag: the loop is not given up in either setting.
            assert!(
                on.ncallee_heads <= 1,
                "[{trace}] flagged at most once: {}",
                on.line()
            );
            assert!(
                on.blacklisted <= off.blacklisted,
                "[{trace}] the loop is not given up: {} vs {}",
                on.blacklisted,
                off.blacklisted
            );
        } else {
            assert_eq!(
                on.ncallee_heads,
                1,
                "[{trace}] the head is flagged once: {}",
                on.line()
            );
            assert!(
                on.blacklisted < off.blacklisted,
                "[{trace}] the loop is not given up: {} vs {}",
                on.blacklisted,
                off.blacklisted
            );
        }
    }
}

/// R1 `rearm`: a main-chunk loop whose callee leaves compiled code every 50th iteration for an `__index` function (`r1_rearm`). Every mode and knob value prints what
/// the interpreter prints; with `rearm=0` nothing is armed, with `rearm=16` the rearmed ticks enter the traced head (`rearm_enters` > 0) and the run executes fewer
/// interpreter bytecodes (only the `on` jobs run with the production sampling tick of 1000 back-edges; `always` / `on2` run with a tick at every back-edge: nothing to
/// rearm there, same output). `r1_rearm_warm`: the exits are in a loop whose head is not traced: the tick is armed but never enters (`rearm_enters` == 0, `rearm_misses` > 0).
/// Hooks never arm (`HOOKMASK` programs of `tr_hook`), GC stress 2 and verified runs. Mutants `rearm_never`, `rearm_warm_credit`, `rearm_any_head`, `rearm_tick_reset`.
#[test]
fn r1_rearm_programs() {
    // (BUNDLE-S: `t1q=1` pins tier 1 to leaving at a Lua `__index` function; with the process default tier 1 runs the handler in compiled code and there is nothing to rearm)
    let mut jobs = Vec::new();
    for prog in ["r1_rearm", "r1_rearm_warm", "r1_rearm_hook", "r1_rearm_co"] {
        for trace in ["always", "on2", "on"] {
            for lim in ["rearm=0,t1q=1", "rearm=1,t1q=1", "rearm=16,t1q=1"] {
                jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                jobs.push(Job::new(prog, trace).limits_s(lim));
            }
            jobs.push(
                Job::new(prog, trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s("rearm=16,t1q=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .opts("+exitstorm=7 +verify +randra=5")
                    .limits_s("rearm=16,t1q=1"),
            );
            jobs.push(Job::new(prog, trace).budget(17).limits_s("rearm=16,t1q=1"));
            jobs.push(
                Job::new(prog, trace)
                    .opts("+emurun +verify")
                    .limits_s("rearm=16,t1q=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("rearm=16,t1q=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s("rearm=16,t1q=1"),
            );
        }
        jobs.push(Job::new(prog, "t1"));
    }
    let res = compare(jobs);
    let find = |prog: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == prog
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| (o.stats.clone(), o.insns))
            .expect("job")
    };
    for trace in ["always", "on2", "on"] {
        let (off, _) = find("r1_rearm", trace, "rearm=0,t1q=1");
        assert_eq!(
            (off.rearm_arms, off.rearm_enters, off.rearm_misses),
            (0, 0, 0),
            "[{trace}] knob off arms nothing"
        );
        let (on, _) = find("r1_rearm", trace, "rearm=16,t1q=1");
        eprintln!(
            "r1_rearm [{trace}] armed {} entered {} missed {}",
            on.rearm_arms, on.rearm_enters, on.rearm_misses
        );
        if trace == "on" {
            // (span of 32 units: the last back-edges of the `__index` function's 40-iteration loop are plain interpreter back-edges that move the countdown, which the next
            // rearmed tick must find restored, not reset)
            let (one, _) = find("r1_rearm", trace, "rearm=1,t1q=1");
            assert!(
                one.rearm_restored > 0,
                "[{trace}] the rearmed ticks restore the formation countdown: {}",
                one.rearm_restored
            );
            assert!(
                on.rearm_arms > 0 && on.rearm_enters > 0,
                "[{trace}] the rearmed tick enters the traced head: {} {} {}",
                on.rearm_arms,
                on.rearm_enters,
                on.rearm_misses
            );
        }
        let (h, _) = find("r1_rearm_hook", trace, "rearm=16,t1q=1");
        assert_eq!(
            (h.rearm_arms, h.rearm_enters, h.rearm_misses),
            (0, 0, 0),
            "[{trace}] a hooked thread never arms"
        );
        let (c, _) = find("r1_rearm_co", trace, "rearm=16,t1q=1");
        eprintln!(
            "r1_rearm_co [{trace}] armed {} entered {} missed {}",
            c.rearm_arms, c.rearm_enters, c.rearm_misses
        );
        if trace == "on" {
            assert!(
                c.rearm_enters > 0,
                "[{trace}] a coroutine's loop is re-entered too: {}",
                c.rearm_enters
            );
        }
        let (w, _) = find("r1_rearm_warm", trace, "rearm=16,t1q=1");
        eprintln!(
            "r1_rearm_warm [{trace}] armed {} entered {} missed {}",
            w.rearm_arms, w.rearm_enters, w.rearm_misses
        );
        if trace == "on" {
            assert_eq!(
                w.rearm_enters, 0,
                "[{trace}] a Warm head is never entered by a rearmed tick"
            );
        }
    }
}

/// C39: `innerretry`. `c39_inner`: the main loop's root records through a callee loop it cannot unroll (`InnerLoop`, twice in a row), then the callee is swapped for a
/// loop-free one. `innerretry=0` leaves the head blacklisted for good; `innerretry=6` gives it retry rounds with growing waits and the round after the swap installs
/// the loop's trace (one more trace installed, one head less blacklisted, `inner_retried` >= 1, bounded by the knob). Same output as the interpreter in every mode. Mutants
/// `inner_retry_never`, `inner_retry_unbounded` (the rule itself: `tr_policy::c39_policy`).
#[test]
fn c39_inner_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "innerretry=0,retf=2",
            "innerretry=6,retf=2",
            "innerretry=1,retf=2",
        ] {
            jobs.push(Job::new("c39_inner", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("c39_inner", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("c39_inner", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("c39_inner", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("c39_inner", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
    }
    jobs.push(Job::new("c39_inner", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "c39_inner"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let (off, on, one) = (
            find(trace, "innerretry=0,retf=2"),
            find(trace, "innerretry=6,retf=2"),
            find(trace, "innerretry=1,retf=2"),
        );
        eprintln!(
            "c39_inner [{trace}] off: {} | on: {} rounds {} | one: {} rounds {}",
            off.line(),
            on.line(),
            on.inner_retried,
            one.line(),
            one.inner_retried
        );
        assert_eq!(off.inner_retried, 0, "[{trace}] knob off");
        assert!(
            on.inner_retried >= 1 && on.inner_retried <= 6,
            "[{trace}] rounds started, bounded by the knob: {}",
            on.inner_retried
        );
        assert!(
            one.inner_retried <= 1,
            "[{trace}] one round at most: {}",
            one.inner_retried
        );
        assert!(
            on.installed > off.installed,
            "[{trace}] a retry round installs the loop's trace after the swap: {} vs {}",
            on.installed,
            off.installed
        );
        assert!(
            on.blacklisted < off.blacklisted,
            "[{trace}] and the head is no longer blacklisted: {} vs {}",
            on.blacklisted,
            off.blacklisted
        );
    }
}

/// BUNDLE-C `sbump`: a stitch exit at the recursion limit gets a side trace on the retry (program `bc_sbump`: a recursive walk over a tree whose depth varies by data,
/// called from a loop). Same output as the interpreter in every mode; `sbump=1` installs at least as many side traces as `sbump=0`, and more in the loop shapes of
/// `always`. Mutant `sbump_never`.
#[test]
fn bc_sbump_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "recnat=1,recany=1,siderooms=1,nilstore=1,typesides=1,ineffcover=1,sidemin=4,sbump=0",
            "recnat=1,recany=1,siderooms=1,nilstore=1,typesides=1,ineffcover=1,sidemin=4,sbump=1",
            "recnat=1,recany=1,siderooms=1,nilstore=1,typesides=1,ineffcover=1,sidemin=4,sbump=2",
        ] {
            jobs.push(Job::new("bc_sbump", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("bc_sbump", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("bc_sbump", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("bc_sbump", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("bc_sbump", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
    }
    jobs.push(Job::new("bc_sbump", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bc_sbump"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    let (mut gained, mut lost) = (false, false);
    for trace in ["always", "on2"] {
        let p = "recnat=1,recany=1,siderooms=1,nilstore=1,typesides=1,ineffcover=1,sidemin=4";
        let (off, on) = (
            find(trace, &format!("{p},sbump=0")),
            find(trace, &format!("{p},sbump=1")),
        );
        eprintln!("bc_sbump [{trace}] off: {} | on: {}", off.line(), on.line());
        gained |= on.side_installed > off.side_installed;
        lost |= on.side_installed < off.side_installed;
    }
    assert!(
        gained && !lost,
        "the retry with one more unroll installs side traces the plain retry cannot"
    );
}

/// BUNDLE-D: the limits of the pieces, each explicit (the struct default of the tests is 0 for all of them).
const BD_LIMITS: [&str; 3] = [
    "ccalls=0,armm=0,sbump2=0",
    "ccalls=1",
    "ccalls=1,armm=1,sbump2=2",
];

/// BUNDLE-D `ccalls`: the C-stack check of an inlined metamethod frame / pcall is inline loads and a compare. Recursion through `__index` / `__add` /
/// `__unm` / `__lt` and `pcall` around every depth near `LUAI_MAXCCALLS` raises the same errors at the same depths as the interpreter, in every trace
/// mode with and without the limit. Mutants `ccalls_inline_off_by_one` (and `ccalls_guard_off` through the slow path).
#[test]
fn bd_ccalls_programs() {
    let mut jobs = Vec::new();
    for prog in ["bd_mm_depth", "g2_pcall_depth"] {
        for trace in ["always", "on2", "on"] {
            for lim in BD_LIMITS {
                for (opts, budget) in [
                    ("", 0),
                    ("+verify", 0),
                    ("+exitstorm=3 +verify", 0),
                    ("", 17),
                    ("-loop", 0),
                ] {
                    let mut j = Job::new(prog, trace).opts(opts).limits_s(lim);
                    if budget != 0 {
                        j = j.budget(budget);
                    }
                    jobs.push(j);
                }
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(Job::new(prog, trace).limits_s(lim).opts("+emurun +verify"));
            }
        }
        jobs.push(Job::new(prog, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() {
            assert!(
                o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.tag(),
                o.stats.line()
            );
        }
    }
    // the inline check answers exactly like the helper: the same entries and exits (an off-by-one admits one depth the helper refuses)
    for (j, o) in &res {
        if j.limits == BD_LIMITS[0] && j.env.is_empty() {
            let other = res.iter().find(|(k, _)| {
                k.prog == j.prog
                    && k.trace == j.trace
                    && k.opts == j.opts
                    && k.budget == j.budget
                    && k.env == j.env
                    && k.limits == BD_LIMITS[1]
            });
            let (_, p) = other.expect("twin job");
            assert!(
                o.stats.entered == p.stats.entered && o.stats.exits == p.stats.exits,
                "{} [{} {}]: ccalls=0 {} | ccalls=1 {}",
                j.prog,
                j.trace,
                j.opts,
                o.stats.line(),
                p.stats.line()
            );
        }
    }
}

/// BUNDLE-D `armm`: tier 1 continues in compiled code after the Lua metamethod of an arithmetic instruction returned (`__add` `__sub` `__mul` `__div` `__unm`
/// and the rest through the `MMBIN*` / `UNM` / `BNOT` path). Same output as the interpreter in every mode: results of every type, plain and nil operands (the
/// interpreter's errors), a metamethod that errors, yields inside the metamethods, vectors with tail-call and plain returns, recursion through `__add`. With
/// `t15_oracle` the instruction count identity holds. Mutants `armm_never` (lowering test `bd_armm_lowering`), `armm_count_skip`, `armm_land_skip`,
/// `armm_pay_skip`, `armm_dst_off`.
#[test]
fn bd_armm_programs() {
    let mut jobs = Vec::new();
    for prog in ["bd_arith_mm", "bd_vec_loop"] {
        for trace in ["always", "on2", "on"] {
            for lim in BD_LIMITS {
                for (opts, budget) in [
                    ("", 0),
                    ("+verify", 0),
                    ("+exitstorm=3 +verify", 0),
                    ("", 3),
                    ("", 17),
                    ("-loop", 0),
                ] {
                    let mut j = Job::new(prog, trace).opts(opts).limits_s(lim);
                    if budget != 0 {
                        j = j.budget(budget);
                    }
                    jobs.push(j);
                }
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "1"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("TR_CORE_GCMODE", "inc")
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("TR_CORE_GCMODE", "gen")
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(Job::new(prog, trace).limits_s(lim).opts("+emurun +verify"));
            }
        }
        jobs.push(Job::new(prog, "t1"));
    }
    #[cfg(feature = "t15_oracle")]
    for prog in ["bd_arith_mm", "bd_vec_loop"] {
        for b in [None, Some(3), Some(17), Some(200)] {
            for trace in ["always", "on2"] {
                let mut j = Job::new(prog, trace).limits_s("ccalls=1,armm=1");
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
        }
    }
    compare(jobs);
}

/// BUNDLE-D `sbump2`: per-exit escalating unroll for side attempts cut by the recursion limits (counter-bounded). Same output as the interpreter in every
/// mode and with the other rules on or off (program `bc_sbump`, `bd_vec_loop` for the recursion through `__add`). Mutants `sbump2_never` and `sbump2_unbounded`
/// are detected by the rule's unit test (`tr_bd::bd_sbump2_rule`).
#[test]
fn bd_sbump2_programs() {
    let mut jobs = Vec::new();
    let p = "recnat=1,recany=1,siderooms=1,nilstore=1,typesides=1,ineffcover=1,sidemin=4";
    for prog in ["bc_sbump", "bd_vec_loop"] {
        for trace in ["always", "on2"] {
            for lim in [
                "sbump=0,sbump2=0",
                "sbump=1,sbump2=1",
                "sbump=1,sbump2=3",
                "sbump=0,sbump2=8",
            ] {
                let l = format!("{p},{lim}");
                jobs.push(Job::new(prog, trace).opts("+verify").limits_s(&l));
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s(&l),
                );
                jobs.push(Job::new(prog, trace).budget(17).limits_s(&l));
                jobs.push(
                    Job::new(prog, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .limits_s(&l),
                );
                jobs.push(Job::new(prog, trace).opts("+emurun +verify").limits_s(&l));
            }
        }
    }
    compare(jobs);
}

/// BUNDLE-C `fnpromo`: a function that is entered from untraced code (a `table.sort` comparator) and never recurses is promoted to a generic function root after its
/// probe budget (program `bc_fnpromo`). Same output as the interpreter in every mode; `fnpromo=0` never promotes, `fnpromo=N` at most N (the rule itself: `tr_policy::bc_policy`).
#[test]
fn bc_fnpromo_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "recnat=1,recany=1,nilstore=1,sidemin=4,fnpromo=0",
            "recnat=1,recany=1,nilstore=1,sidemin=4,fnpromo=1,fnpromow=0",
            "recnat=1,recany=1,nilstore=1,sidemin=4,fnpromo=6,fnpromow=0",
        ] {
            jobs.push(Job::new("bc_fnpromo", trace).opts("+verify").limits_s(lim));
            jobs.push(
                Job::new("bc_fnpromo", trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(lim),
            );
            jobs.push(Job::new("bc_fnpromo", trace).budget(17).limits_s(lim));
            jobs.push(
                Job::new("bc_fnpromo", trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(lim),
            );
            jobs.push(
                Job::new("bc_fnpromo", trace)
                    .opts("+emurun +verify")
                    .limits_s(lim),
            );
        }
    }
    jobs.push(Job::new("bc_fnpromo", "t1"));
    let res = compare(jobs);
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bc_fnpromo"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let p = "recnat=1,recany=1,nilstore=1,sidemin=4";
        let (off, one, six) = (
            find(trace, &format!("{p},fnpromo=0")),
            find(trace, &format!("{p},fnpromo=1,fnpromow=0")),
            find(trace, &format!("{p},fnpromo=6,fnpromow=0")),
        );
        eprintln!(
            "bc_fnpromo [{trace}] off: {} | one: {} promoted {} | six: {} promoted {}",
            off.line(),
            one.line(),
            one.fn_promoted,
            six.line(),
            six.fn_promoted
        );
        assert_eq!(off.fn_promoted, 0, "[{trace}] knob off");
        assert!(
            one.fn_promoted <= 1,
            "[{trace}] the cap of one promotion: {}",
            one.fn_promoted
        );
        assert!(
            six.fn_promoted <= 6,
            "[{trace}] bounded by the cap: {}",
            six.fn_promoted
        );
    }
}

/// BUNDLE-GM: the `gmatch` iterator step of a generic `for` recorded as a `Lib` helper call (limit `gmatch`; program `gm_loops`). Same output as the
/// interpreter in every mode (lockstep, exit storms, budgets, GC stress 1 and 2, emulator, the stack-move stress); (K3b: `gmatch` is hard-wired on) traces are
/// installed for the loops and (almost) no `CFunc` abort happens. Mutants `gm_kind_skip`, `gm_no_commit`, `gm_end_guard_skip`.
#[test]
fn gm_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 3),
            ("", 17),
            ("-loop", 0),
            ("+emurun +verify", 0),
            ("-fwd -cse -loop -sink", 0),
            ("+membudget +verify", 0),
        ] {
            let mut j = Job::new("gm_loops", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = String::new();
            jobs.push(j);
        }
        for gc in ["1", "2"] {
            let mut j = Job::new("gm_loops", trace).env("RUSTYNAIL_GC_STRESS", gc);
            j.limits = String::new();
            jobs.push(j);
        }
        let mut j = Job::new("gm_loops", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = String::new();
        jobs.push(j);
    }
    jobs.push(Job::new("gm_loops", "t1"));
    let res = compare(jobs);
    let mut on = 0u64;
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits.is_empty() {
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "traces did not run: {}",
                o.stats.line()
            );
        }
        if j.trace == "always" && j.opts.is_empty() && j.env.is_empty() && j.budget.is_none() {
            let n = o
                .stats
                .aborts
                .iter()
                .filter(|(k, _)| k.starts_with("CFunc"))
                .map(|(_, v)| *v)
                .sum::<u64>();
            on += n;
        }
    }
    eprintln!("gm_loops: CFunc aborts {on}");
    assert!(
        on <= 200,
        "the gmatch recorder must remove CFunc aborts: {on}"
    );
}

/// C38 slice 1: `sidenest`. A side recording that reaches a callee loop head with a root trace ends with the frame-linking exit (programs `c38_side`, `c38_side2`).
/// Same output as the interpreter in every mode; `sidenest=0` never ends a side trace that way, `sidenest=1` does (stat `sidelinks`). Mutant `side_frame_link_never`.
#[test]
fn c38_side_programs() {
    let mut jobs = Vec::new();
    for p in ["c38_side", "c38_side2"] {
        for trace in ["always", "on2"] {
            for lim in [
                "sidenest=0",
                "sidenest=1",
                "sidenest=1,loopany=0",
                "sidenest=1,nest=2",
            ] {
                jobs.push(Job::new(p, trace).opts("+verify").limits_s(lim));
                jobs.push(
                    Job::new(p, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s(lim),
                );
                jobs.push(Job::new(p, trace).budget(17).limits_s(lim));
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .limits_s(lim),
                );
                jobs.push(Job::new(p, trace).opts("+emurun +verify").limits_s(lim));
            }
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    let find = |p: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == p
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for p in ["c38_side", "c38_side2"] {
        for trace in ["always", "on2"] {
            let (off, on) = (find(p, trace, "sidenest=0"), find(p, trace, "sidenest=1"));
            eprintln!(
                "{p} [{trace}] off: {} | on: {} sidelinks {}",
                off.line(),
                on.line(),
                on.side_links
            );
            assert_eq!(off.side_links, 0, "{p} [{trace}] knob off");
            if p == "c38_side" {
                assert!(
                    on.side_links >= 1,
                    "{p} [{trace}]: no side trace ends with a linking exit: {}",
                    on.line()
                );
            }
        }
    }
}

/// BUNDLE-O: polymorphic call sites (program `po_poly`: 2..5 receiver classes, a changing set, a replaced method, closures of one prototype with
/// different upvalues, `__index` chains of different depth). Same output as the interpreter in every mode, with the family cap small
/// (`maxside=2`) and the cap raised for exits that keep coming back (`polyside`, `polywait`): the raised cap installs more side traces than the
/// hard cap, and the hard cap (`polyside=0`) stays hard. (K3a: every job names `ncallee=0` and, unless it sets them, `polyframes=0`: with the shipped `ncallee=1` / `polyframes=2` this program's polymorphic call sites are served by callee-guard chains inside the frame limit and the raised cap installs nothing more.) Mutants `polyside_default_ignored` (process default, see `tr_policy`) and `polywait_ignored`.
#[test]
fn po_poly_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for lim in [
            "",
            "maxside=2,polyframes=0,ncallee=0",
            "maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0",
            "maxside=2,polyside=6,polywait=3,polyframes=0,ncallee=0",
            "maxside=1,polyside=2,polywait=2,polyframes=0,ncallee=0",
            "maxside=2,polyside=0,polyframes=0,ncallee=0",
        ] {
            let mut j = Job::new("po_poly", trace).opts("+verify");
            j.limits = lim.into();
            jobs.push(j);
        }
        for lim in [
            "maxside=2,polyside=6,polywait=200,polyframes=0,ncallee=0",
            "maxside=2,polyside=6,polywait=1,polyframes=9,ncallee=0",
        ] {
            let mut j = Job::new("po_poly", trace).opts("+verify");
            j.limits = lim.into();
            jobs.push(j);
        }
        jobs.push(
            Job::new("po_poly", trace)
                .opts("+exitstorm=3 +verify")
                .limits_s("maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0"),
        );
        jobs.push(
            Job::new("po_poly", trace)
                .budget(17)
                .limits_s("maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0"),
        );
        jobs.push(
            Job::new("po_poly", trace)
                .env("RUSTYNAIL_GC_STRESS", "2")
                .limits_s("maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0"),
        );
        jobs.push(
            Job::new("po_poly", trace)
                .opts("+emurun +verify")
                .limits_s("maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0"),
        );
    }
    jobs.push(Job::new("po_poly", "t1"));
    for trace in ["always", "on2"] {
        for lim in [
            "maxside=2,polyside=0,polyframes=0,ncallee=0",
            "maxside=2,polyside=6,polywait=1,polyframes=2,ncallee=0",
        ] {
            let mut j = Job::new("po_branch", trace).opts("+verify");
            j.limits = lim.into();
            jobs.push(j);
        }
    }
    let res = compare(jobs);
    for trace in ["always", "on2"] {
        let g = |lim: &str| {
            res.iter()
                .find(|(j, _)| j.prog == "po_branch" && j.trace == trace && j.limits == lim)
                .map(|(_, o)| o.stats.clone())
                .expect("job")
        };
        let (hard, poly) = (
            g("maxside=2,polyside=0,polyframes=0,ncallee=0"),
            g("maxside=2,polyside=6,polywait=1,polyframes=2,ncallee=0"),
        );
        assert!(
            hard.side_installed >= 1,
            "[{trace}] po_branch: {}",
            hard.line()
        );
        assert_eq!(
            poly.side_installed,
            hard.side_installed,
            "[{trace}] branch exits keep the hard cap: {} vs {}",
            poly.line(),
            hard.line()
        );
    }
    let find = |trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "po_poly"
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for trace in ["always", "on2"] {
        let hard = find(trace, "maxside=2,polyside=0,polyframes=0,ncallee=0");
        let poly = find(
            trace,
            "maxside=2,polyside=6,polywait=1,polyframes=0,ncallee=0",
        );
        let wait = find(
            trace,
            "maxside=2,polyside=6,polywait=3,polyframes=0,ncallee=0",
        );
        assert!(
            poly.side_installed > hard.side_installed,
            "[{trace}] polyside raises the cap: {} vs {}",
            poly.line(),
            hard.line()
        );
        let never = find(
            trace,
            "maxside=2,polyside=6,polywait=200,polyframes=0,ncallee=0",
        );
        assert!(never.side_installed <= hard.side_installed, "[{trace}] an exit that has not been deferred polywait times gets no side trace beyond the cap: {} vs {}", never.line(), hard.line());
        let shallow = find(
            trace,
            "maxside=2,polyside=6,polywait=1,polyframes=9,ncallee=0",
        );
        assert!(
            shallow.side_installed <= hard.side_installed,
            "[{trace}] no exit lies in 9 inlined frames: {} vs {}",
            shallow.line(),
            hard.line()
        );
        assert!(
            poly.side_installed >= wait.side_installed,
            "[{trace}] polywait delays: {} vs {}",
            poly.line(),
            wait.line()
        );
    }
}

/// N3b: `setmetatable(t, mt)` is a recorded `SETMT` intrinsic and `return setmetatable(...)` / `return <recorded native>(...)` is a
/// call plus a return. The programs run with the interpreter's output in every mode (verify, exit storms, register eviction, emulator,
/// GC stress 2, without the loop optimiser / forwarding, small budgets): metatables present / protected
/// / with `__gc` / `__index` chains, nil metatables, wrong argument kinds (the interpreter's error texts), coroutines, the collector
/// barrier. Detector of the mutants `setmt_protect_skip`, `setmt_gc_skip`, `setmt_barrier_skip` and `setmt_result_mt`.
#[test]
fn setmt_programs() {
    let mut jobs = Vec::new();
    for p in [
        "n3b_setmt",
        "n3b_setmt_err",
        "n3b_setmt_gc",
        "n3b_setmt_co",
        "n3b_setmt_gc_exit",
        "n3b_setmt_gc_exit2",
        "a1_setmt_old",
    ] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
            for gm in ["gen", "inc"] {
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("TR_CORE_GCMODE", gm),
                );
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "1")
                        .env("TR_CORE_GCMODE", gm),
                );
                // (the invariant check after every step: a black table that points to a white metatable is found at once)
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("RUSTYNAIL_GC_VERIFY", "1")
                        .env("TR_CORE_GCMODE", gm),
                );
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_VERIFY", "1")
                        .env("TR_CORE_GCMODE", gm),
                );
            }
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n3b_setmt"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                !o.stats.aborts.contains_key("TailCall") && !o.stats.aborts.contains_key("Diverge"),
                "{}: {}",
                j.prog,
                o.stats.line()
            );
        }
        if (j.prog == "n3b_setmt_gc_exit" || j.prog == "n3b_setmt_gc_exit2")
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            // the helper refuses a metatable with `__gc`: every iteration after it appeared leaves the trace
            assert!(
                o.stats.exits >= 300,
                "{}: the trace did not leave at the __gc metatable: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// The inline `#t` of traces (`TLEN`) and of tier 1 (`LEN`): stack push / pop, stale hints, holes, hash-part integers, array boundaries, `__len`,
/// no array part, a metatable appearing in mid-loop, in every trace mode and with the collector stressed. Detector of the mutants
/// `tlen_inline_prev_skip`, `tlen_inline_next_skip`, `tlen_inline_dummy_skip` and `tlen_inline_mt_skip` (the last one tier 1 only).
#[test]
fn tlen_inline_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        jobs.push(Job::new("tlen_inline", trace).opts("+verify"));
        jobs.push(Job::new("tlen_inline", trace).opts("+exitstorm=3 +verify"));
        jobs.push(Job::new("tlen_inline", trace).opts("+exitstorm=7 +verify +randra=5"));
        jobs.push(Job::new("tlen_inline", trace).budget(17));
        jobs.push(Job::new("tlen_inline", trace).opts("-loop"));
        jobs.push(Job::new("tlen_inline", trace).opts("-fwd -cse -loop -sink"));
        jobs.push(Job::new("tlen_inline", trace).opts("+emurun +verify"));
        jobs.push(Job::new("tlen_inline", trace).env("RUSTYNAIL_GC_STRESS", "2"));
    }
    jobs.push(Job::new("tlen_inline", "t1"));
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// F1: the folds and intrinsics of F1 (`x ^ 2` as a product, `math.sin` / `cos` / `exp` / `log`, `math.min` / `math.max` of three or more
/// arguments, `string.len` / `s:len()`) and the carried `next` index (`pairs` / `next` order, assigning nil to existing fields, stale hints,
/// nested and interleaved traversals, errors) in every trace mode against the interpreter. Detector of the
/// mutants `f1_pow_any`, `f1_minmax_order`, `f1_strlen_off`, `f1_log_base`.
#[test]
fn f1_programs() {
    let mut jobs = Vec::new();
    for p in ["f1_folds", "f1_next_hint"] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
            for gm in ["gen", "inc"] {
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("TR_CORE_GCMODE", gm),
                );
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "1")
                        .env("TR_CORE_GCMODE", gm),
                );
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_VERIFY", "1")
                        .env("TR_CORE_GCMODE", gm),
                );
            }
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "f1_folds"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                !o.stats.aborts.contains_key("Diverge"),
                "{}: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// A1: the `NEWREF` of a constant key into a table the trace allocated is a few stores at nodes known at record time (A1): the
/// constructor shapes of `a1_inline_newref` (1 to 7 keys with collisions, 9 keys, array parts, collectable values, a type change in the
/// middle of a constructor, metatables built by constructors, an integer key between the insertions, rehash and deletion) in every
/// trace mode against the interpreter. Detector of the mutants `nr_key_skip`, `nr_next_skip`,
/// and `nr_flags_skip`.
#[test]
fn a1_inline_newref() {
    let mut jobs = Vec::new();
    let p = "a1_inline_newref";
    for trace in ["always", "on2"] {
        jobs.push(Job::new(p, trace).opts("+verify"));
        jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
        jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
        jobs.push(Job::new(p, trace).budget(17));
        jobs.push(Job::new(p, trace).opts("-loop"));
        jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
        jobs.push(Job::new(p, trace).opts("+emurun +verify"));
        jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        for gm in ["gen", "inc"] {
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("TR_CORE_GCMODE", gm),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .env("TR_CORE_GCMODE", gm),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .env("TR_CORE_GCMODE", gm),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .env("TR_CORE_GCMODE", gm),
            );
        }
    }
    jobs.push(Job::new(p, "t1"));
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() && j.limits.is_empty() {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// S1: hot guard exits inside inlined frames (plain, tail and metamethod frames, depth 1..4, closures of one prototype with different
/// upvalues, result adjustments, errors with tracebacks, a coroutine, stack growth under virtual frames, a hook installed in the middle,
/// eight-way branching, argument type changes, vararg frames that are kept out) get side traces whose frames stay virtual: the
/// results equal the interpreter's in every mode (and at least six side traces are installed). Detector of the
/// S1 mutants (frame cells, virtual restore, recorder seeding, count model of a mid-frame start).
#[test]
fn s1_frame_side_programs() {
    let mut jobs = Vec::new();
    let p = "s1_frame_sides";
    for trace in ["always", "on2"] {
        jobs.push(Job::new(p, trace).opts("+verify"));
        jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
        jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
        jobs.push(Job::new(p, trace).budget(3));
        jobs.push(Job::new(p, trace).budget(17));
        jobs.push(Job::new(p, trace).opts("-loop"));
        jobs.push(Job::new(p, trace).opts("-fwd -cse -loop -sink"));
        jobs.push(Job::new(p, trace).opts("+emurun +verify"));
        // (+emuverify: every exit goes through the handler, which unwinds the frames of an exit that has a side trace)
        jobs.push(Job::new(p, trace).opts("+emuverify"));
        jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
        jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
        for gm in ["gen", "inc"] {
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("TR_CORE_GCMODE", gm),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .env("TR_CORE_GCMODE", gm),
            );
            jobs.push(
                Job::new(p, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .env("TR_CORE_GCMODE", gm),
            );
        }
        // (BUNDLE-M: `siderooms=0` is the control where a side trace that needs more room than its parent guarantees is refused; the mutant `fside_room_ignored` still survives, see docs/JIT_TESTING.md)
        for l in [
            "maxside=2",
            "maxdepth=2",
            "nest=2",
            "unroll=0",
            "siderooms=0",
            "siderooms=0,maxdepth=2",
        ] {
            let mut j = Job::new(p, trace);
            j.limits = l.into();
            jobs.push(j);
        }
    }
    jobs.push(Job::new(p, "t1"));
    let res = compare(jobs);
    let with = res
        .iter()
        .find(|(j, _)| {
            j.trace == "always" && j.limits.is_empty() && j.opts == "+verify" && j.env.is_empty()
        })
        .map(|(_, o)| o.stats.side_installed)
        .expect("job");
    assert!(with >= 6, "s1_frame_sides: the exits inside inlined frames got no side traces: {with} side traces installed");
}

/// N3b: a loop whose metamethod allocates/// N3b: a loop whose metamethod allocates (the collector steps every few dozen iterations; each step is an allocation exit of the trace,
/// which the interpreter serves) keeps running in the trace: the exit-storm rule does not take periodic exits for a storm (the room
/// between collector steps is large), the monitor's entry back-off does not outlive the trace's installation, and the exit re-arms the
/// loop tick so the next back-edge enters again. (Policy code, no mutants: `alloc_storm` room check, `Mon::reset` at install, `arm_tick_next`.)
#[test]
fn mm_alloc_loop_stays_in_the_trace() {
    for gm in ["gen", "inc"] {
        let mut j = Job::new("n3b_mm_alloc", "on2").env("TR_CORE_GCMODE", gm);
        j.opts = "".into();
        let r = compare(vec![j]).remove(0).1;
        assert!(
            r.stats.installed >= 1 && r.stats.ineffective == 0 && r.stats.blacklisted == 0,
            "n3b_mm_alloc [{gm}]: the trace was flushed as ineffective: {}",
            r.stats.line()
        );
        // (BUNDLE-A item 1: a generational run steps the collector several times less often with the larger nursery: 144 allocation exits
        // and 145 entries instead of at least 300; a loop that does not get back into the trace has a handful of entries)
        assert!(r.stats.entered >= 100, "n3b_mm_alloc [{gm}]: the loop did not get back into the trace after the allocation exits: {}", r.stats.line());
    }
}

/// N3: the instruction count identity (9.5) holds for the metamethod programs: the `MMBIN*` behind a failed arithmetic instruction is one
/// fetched instruction that tier 1 never fetches. Detector of the mutant `mm_follower_free`.
#[cfg(feature = "t15_oracle")]
#[test]
fn mm_count_identity() {
    let mut jobs = Vec::new();
    for p in [
        "n3_mm_arith",
        "n3_mm_swap",
        "n3_mm_err",
        "n3_mm_exit",
        "n3_mm_cmp",
        "n3_mm_idx",
        "n3_mm_idx2",
    ] {
        for b in [None, Some(3), Some(17)] {
            for (t, o) in [("always", ""), ("always", "+exitstorm=7"), ("on2", "")] {
                let mut j = Job::new(p, t).opts(o);
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
        }
    }
    let n = jobs.len();
    let res = run_all(jobs);
    let bad: Vec<String> = res
        .iter()
        .filter_map(|(j, o)| ident_check(o).err().map(|e| format!("{}: {e}", j.tag())))
        .collect();
    assert!(
        bad.is_empty(),
        "{} of {n} identity failures:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The nests `for r = 1, N do for i, v in ipairs(t) do ... end end` (and `pairs`) stay in traces: the outer loop records the call of
/// `ipairs` / `pairs`, `TFORPREP` and the first iteration and links to the inner loop's trace; the end of the inner loop is a side trace
/// that continues behind it and links back to the outer loop. Before N7 the outer head aborted `CFunc`, went `Black` and the nest rule
/// flushed the inner trace. Detector of the mutant `tnext_key_site_type` (the traversal end of a `pairs` loop was a type-guard exit: the
/// trace was flushed and re-recorded for ever) and of a regression of the nest rule.
#[test]
fn gfor_nest_stays_in_traces() {
    // (K3b: `typesides=0` is the row that detects `tnext_key_site_type`: with the shipped `typesides=1` a type-guard exit gets a side trace and the
    // misclassified traversal end is not observable in these counters)
    // (BUNDLE-H: with `tnextinl` the traversal loop peels and the misclassified end is no longer visible in these counters: the `typesides=0` rows also run without it)
    for (trace, lim) in [
        ("always", ""),
        ("on2", ""),
        ("always", "typesides=0"),
        ("on2", "typesides=0"),
        ("always", "typesides=0,tnextinl=0"),
        ("on2", "typesides=0,tnextinl=0"),
    ] {
        let r = compare(vec![Job::new("n7_nest_hot", trace)
            .opts("+verify")
            .limits_s(lim)])
        .remove(0)
        .1;
        assert!(
            r.stats.aborts.get("CFunc").copied().unwrap_or(0) == 0,
            "n7_nest_hot [{trace}]: CFunc aborts: {}",
            r.stats.line()
        );
        // (`exits` counts the exits that a side trace takes over: the figure to watch is that no head went `Black`, nothing was flushed
        // for another reason than the end of the run, and every loop end has its side trace)
        assert!(
            r.stats.blacklisted == 0 && r.stats.rerecord == 0 && r.stats.entered >= 1,
            "n7_nest_hot [{trace}]: a head of the nests went Black / was re-recorded: {}",
            r.stats.line()
        );
        // (5.4 records the call and `TFORPREP` only: the loop ends stay `LeaveLoop` side aborts)
        assert!(
            cfg!(rustynail_lua54) || r.stats.side_installed >= 3,
            "n7_nest_hot [{trace}]: no side trace behind the loop end: {}",
            r.stats.line()
        );
    }
}

/// `t[k] = (t[k] or 0) + 1` with a computed key: the programs run with the interpreter's output in every mode (verify, exit storms,
/// register eviction, emulator, GC stress 2, without the loop optimiser / forwarding, small budgets, `HREFV` off).
/// Detector of the mutants `alias_hrefv_as_no` (two refs of one string: a store forwarded past a load of the same key),
/// `alias_hrefv_key_as_must` (two keys of one table taken for one location) and `hrefv_meta_skip` (a present nil-valued key
/// read after a metatable with `__index` appeared).
#[test]
fn dynamic_key_programs() {
    let mut jobs = Vec::new();
    for p in ["n6_dynkey_basic", "n6_dynkey_shape"] {
        for trace in ["always", "on2"] {
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("-fwd -cse -sink"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_HREFV", "0"));
            // (K3b: `dynmask=0` is the lsizenode-guarded form of `HREFV`, the path of the mutants `hrefv_meta_skip` / `hrefv_mask_off_by_one`; the shipped `dynmask=1` walks differently)
            jobs.push(Job::new(p, trace).opts("+verify").limits_s("dynmask=0"));
            jobs.push(Job::new(p, trace).budget(17).limits_s("dynmask=0"));
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "n6_dynkey_basic"
            && j.trace == "always"
            && j.opts == "+verify"
            && j.env.is_empty()
            && j.limits.is_empty()
        {
            assert!(
                o.stats.installed >= 1 && (o.stats.entered >= 1 || o.stats.exits > 0),
                "{}: no trace ran: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// One trace serves every key: the exits of a loop over 64 / 200 distinct keys stay a handful (a per-key side trace chain would exit
/// thousands of times). Detector of `hrefv_chain_stops_early` and `hrefv_mask_off_by_one` (a key behind another in its chain, or at
/// a main position the wrong mask does not reach, is a run-time miss: an exit).
#[test]
fn dynamic_key_loops_do_not_exit_per_key() {
    let res = compare(vec![
        Job::new("n6_dynkey_hot", "always").opts("+verify"),
        Job::new("n6_dynkey_hot", "always"),
        Job::new("n6_dynkey_hot", "on2"),
        // (K3b: the lsizenode-guarded form `dynmask=0`, where `hrefv_mask_off_by_one` lives)
        Job::new("n6_dynkey_hot", "always")
            .opts("+verify")
            .limits_s("dynmask=0"),
        Job::new("n6_dynkey_hot", "on2").limits_s("dynmask=0"),
    ]);
    for (j, o) in &res {
        assert!(
            o.stats.installed >= 1,
            "{} {}: {}",
            j.prog,
            j.trace,
            o.stats.line()
        );
        assert!(
            o.stats.exits < 60,
            "{} {} {}: too many exits: {}",
            j.prog,
            j.trace,
            j.opts,
            o.stats.line()
        );
    }
}

/// R1: the inline `TAILCALL` of tier 1 and the loop-entry rule of the back-off monitor, in the trace modes
/// `always` / `on2` / `on` with and without `+verify`, `+emurun`, small budgets, exit storms, GC stress 1 and 2 (both collector
/// modes), the stack-move stress, hooks, coroutines. Mutants
/// `tailfast_args_skip`, `tailfast_copy_short`, `tailfast_status_skip`.
#[test]
fn r1_tail_and_call_loops() {
    let mut jobs = Vec::new();
    for p in ["r1_tailcalls", "r1_tail_co", "r1_call_loops"] {
        for trace in ["always", "on2", "on"] {
            jobs.push(Job::new(p, trace));
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(3));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "1"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
            for gm in ["gen", "inc"] {
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("TR_CORE_GCMODE", gm),
                );
            }
            for l in ["recnat=1"] {
                let mut j = Job::new(p, trace);
                j.limits = l.into();
                jobs.push(j);
            }
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "r1_tailcalls"
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.limits.is_empty()
            && j.budget.is_none()
        {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// M1: a tree-walking interpreter shape (mutual recursion `eval` / `exec`, `RETURN` / `TAILCALL` with the close flag, nil array
/// elements, errors, stack overflow, coroutines, hooks) in the trace modes with and without `+verify`, `+emurun`, small budgets,
/// exit storms, GC stress 1 and 2 (both collector modes), the stack-move stress, and the limits `unroll=1/3`, `maxdepth=2` as controls. Mutants `side_stitch_never`, `tailfast_close_skip`, `aload_nil_skip`, `depth_stitch_never`.
#[test]
fn m1_interp_shapes() {
    let mut jobs = Vec::new();
    for p in ["m1_interp", "m1_gfor_inl", "m1_rd", "m1_deep"] {
        for trace in ["always", "on2", "on"] {
            jobs.push(Job::new(p, trace));
            jobs.push(Job::new(p, trace).opts("+verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=3 +verify"));
            jobs.push(Job::new(p, trace).opts("+exitstorm=7 +verify +randra=5"));
            jobs.push(Job::new(p, trace).budget(3));
            jobs.push(Job::new(p, trace).budget(17));
            jobs.push(Job::new(p, trace).opts("-loop"));
            jobs.push(Job::new(p, trace).opts("+emurun +verify"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "2"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", "1"));
            jobs.push(Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove"));
            for gm in ["gen", "inc"] {
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("TR_CORE_GCMODE", gm),
                );
            }
            for l in [
                "loopany=1",
                "unroll=1",
                "unroll=3",
                "unroll=3,maxdepth=2",
                "recnat=0",
                "fnroot=1",
                "maxside=64",
            ] {
                let mut j = Job::new(p, trace);
                j.limits = l.into();
                jobs.push(j);
            }
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if (j.prog == "m1_gfor_inl" || j.prog == "m1_rd" || j.prog == "m1_deep")
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.limits.is_empty()
            && j.budget.is_none()
        {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.prog,
                o.stats.line()
            );
            // M1: the inlining depth limit stitches (`Depth` aborts otherwise), inlined `ipairs` loops record (`Nyi(Tforprep)` otherwise)
            if j.prog == "m1_deep" {
                assert!(
                    o.stats.stitches >= 1,
                    "{}: no stitching trace: {}",
                    j.prog,
                    o.stats.line()
                );
            }
            if !cfg!(rustynail_lua54) {
                assert!(
                    !o.stats.aborts.contains_key("Nyi(Tforprep)"),
                    "{}: TFORPREP aborts: {}",
                    j.prog,
                    o.stats.line()
                );
            }
        }
        if j.prog == "m1_interp"
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.limits.is_empty()
            && j.budget.is_none()
        {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.prog,
                o.stats.line()
            );
            // (H1: 73 (5.4: 8) before `framemin` refused the one-`RETURN` side traces of inlined frames and `fck` delayed the frame side traces
            // of function-entry / continuation families)
            // M1: side recordings stitch at recursive calls; the `RETURN k`
            // of the evaluators records at all (`Nyi(Return)` otherwise)
            // (K3a: 65 with the old cap `maxside=64`, see the next block; the shipped `maxside=16` installs 37 here)
            assert!(
                o.stats.side_installed >= if cfg!(rustynail_lua54) { 4 } else { 30 },
                "{}: side traces: {}",
                j.prog,
                o.stats.line()
            );
            assert!(
                !o.stats.aborts.contains_key("Nyi(Return)"),
                "{}: RETURN k aborts: {}",
                j.prog,
                o.stats.line()
            );
        }
        if j.prog == "m1_interp"
            && j.trace == "always"
            && j.opts.is_empty()
            && j.env.is_empty()
            && j.limits == "maxside=64"
            && j.budget.is_none()
        {
            assert!(
                o.stats.side_installed >= if cfg!(rustynail_lua54) { 4 } else { 65 },
                "{}: side traces with maxside=64: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
    // M1 `sidestitch`: side recordings stitch at recursive calls, which the mutant `side_stitch_never` turns into `Recursion` aborts
    // (C43 runner survivor: nothing compared the two)
    let pick = |lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "m1_interp"
                    && j.trace == "always"
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
                    && j.limits == lim
            })
            .map(|(_, o)| o.stats.clone())
    };
    if let Some(on) = pick("") {
        let rec = |s: &TStats| s.side_aborts.get("Recursion").copied().unwrap_or(0);
        assert!(
            rec(&on) <= 4,
            "side recordings must stitch at recursive calls instead of aborting `Recursion`: {}",
            on.line()
        );
    }
}

/// BUNDLE-1 matrix: the programs in every trace mode with exit storms, budgets, GC stress and the stack-move stress, once per knob setting
/// (`limits`), against the interpreter run.
fn b1_jobs(progs: &[&str], limits: &[&str]) -> Vec<Job> {
    let mut jobs = Vec::new();
    for p in progs {
        for l in limits {
            for trace in ["always", "on2", "on"] {
                for (opts, budget) in [
                    ("", 0),
                    ("+verify", 0),
                    ("+exitstorm=3 +verify", 0),
                    ("+exitstorm=7 +verify +randra=5", 0),
                    ("", 3),
                    ("", 17),
                    ("-loop", 0),
                ] {
                    let mut j = Job::new(p, trace).opts(opts);
                    if budget != 0 {
                        j = j.budget(budget);
                    }
                    j.limits = (*l).into();
                    jobs.push(j);
                }
                for gc in ["1", "2"] {
                    let mut j = Job::new(p, trace).env("RUSTYNAIL_GC_STRESS", gc);
                    j.limits = (*l).into();
                    jobs.push(j);
                }
                let mut j = Job::new(p, trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
                j.limits = (*l).into();
                jobs.push(j);
            }
        }
    }
    jobs
}

/// BUNDLE-1 (1): `TrLimits::wgate` (the deterministic start gate: back-edges weighted by the loop's bytecodes) parses, is 0 in the plain
/// configuration, a gate beyond the program's work records nothing and a small one only delays the first recording.
#[test]
fn b1_wgate_gate_counts_work() {
    assert_eq!(TrLimits::default().wgate, 0);
    assert_eq!(TrLimits::parse("wgate=7").0.wgate, 7);
    assert!(rn_jit::tr::cache::WGATE_PROCESS > 0);
    let mut jobs = Vec::new();
    for w in ["wgate=4000000000", "wgate=2", "wgate=0"] {
        let mut j = Job::new("int_sum", "on2").opts("+verify");
        j.limits = format!("{w},fnroot=0");
        jobs.push(j);
    }
    let res = compare(jobs);
    assert_eq!(
        res[0].1.stats.recorded,
        0,
        "a gate beyond the program's work: {}",
        res[0].1.stats.line()
    );
    assert!(
        res[1].1.stats.installed >= 1,
        "wgate=2 only delays: {}",
        res[1].1.stats.line()
    );
    assert!(res[2].1.stats.installed >= 1);
}

/// BUNDLE-1 (2): tail-called functions count and compile (`tailhook`): the programs run identically in every mode with the knob on, at
/// the compile-only level, and off (control).
/// Mutants `tailhook_ignored`, `tailhook_from_jit_enter`.
#[test]
fn b1_tail_programs() {
    let res = compare(b1_jobs(
        &["b1_tail"],
        &["tailhook=1", "tailhook=0", "tailhook=2"],
    ));
    assert!(!res.is_empty());
}

/// B3-4: `CONCAT` with number operands is one helper (`cnum`: no `SNEW` per number, the converted strings are never interned); the
/// concatenation programs run identically in every mode (K3b: `cnum` is hard-wired on).
#[test]
fn b3_cnum_programs() {
    let res = compare(b1_jobs(&["str_concat", "str_temporaries"], &[""]));
    assert!(!res.is_empty());
}

/// BUNDLE-1 (4): loops with string lengths and read-only library calls are peeled (`hdrlib`), also when the string metatable and the strings
/// change; the knob off is the control. Mutants `hdrlib_never`, `hdrlib_table_tlen`, `hdrlib_setmt`, `hdrlib_newref`, `hdrlib_third_stale`.
#[test]
fn b1_hdr_programs() {
    let res = compare(b1_jobs(&["b1_hdr"], &["hdrlib=1", "hdrlib=0"]));
    assert!(!res.is_empty());
}

/// HDRSUB (old B8): loops with `string.sub` / `string.char` / `tostring` are peeled with `hdrlib` 2 / 3 / 4 (header-safe allocating library
/// calls) and the lean entry (K3b: `libsub` is hard-wired on): the ring-buffer program in every trace mode with verification, exit storms, budgets, GC stress
/// 1 and 2 in both collector modes (and the heap verifier), the stack-move stress and the emulator; the knobs off are the controls.
/// Mutants `hdrlib_sub_phi_stale`, `sub_start_off`, `lib_kind_skip`, `lib_bytes_small`, `lib_no_gc_check`.
#[test]
fn hs_sub_ring_matrix() {
    let mut jobs = Vec::new();
    for l in ["hdrlib=2", "hdrlib=4", "hdrlib=1"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("+emurun +verify", 0),
            ] {
                let mut j = Job::new("b8_sub_ring", trace).opts(opts).limits_s(l);
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                for mode in ["inc", "gen"] {
                    jobs.push(
                        Job::new("b8_sub_ring", trace)
                            .limits_s(l)
                            .env("RUSTYNAIL_GC_STRESS", gc)
                            .env("TR_CORE_GCMODE", mode),
                    );
                }
            }
            jobs.push(
                Job::new("b8_sub_ring", trace)
                    .limits_s(l)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .env("TR_CORE_GCMODE", "gen"),
            );
            jobs.push(
                Job::new("b8_sub_ring", trace)
                    .limits_s(l)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
            );
        }
    }
    let res = compare(jobs);
    assert!(!res.is_empty());
}

/// BUNDLE-A: `tonumber(string)` (`libnum`), the plain call `next(t [, k])` (`libnext`) and plain `string.find` (`libfind`) recorded: the three programs in every
/// trace mode with verification, exit storms, budgets, GC stress 1 and 2 in both collector modes (and the heap verifier), the stack-move stress and the
/// emulator, with each knob alone and all together (the knobs off are the controls). Mutants `tonum_type_guard_skip`, `tonum_mode_ignored`, `next_ctl_nil`,
/// `next_callee_guard_skip`, `find_specials_skip`, `find_end_off`, `find_init_neg_skip`, `find_outcome_guard_skip`.
fn ba_jobs(prog: &str, lims: &[&str]) -> Vec<Job> {
    let mut jobs = Vec::new();
    for l in lims {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("+exitstorm=7 +verify +randra=5", 0),
                ("", 3),
                ("", 17),
                ("-loop", 0),
                ("+emurun +verify", 0),
            ] {
                let mut j = Job::new(prog, trace).opts(opts).limits_s(l);
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            for gc in ["1", "2"] {
                for mode in ["inc", "gen"] {
                    jobs.push(
                        Job::new(prog, trace)
                            .limits_s(l)
                            .env("RUSTYNAIL_GC_STRESS", gc)
                            .env("TR_CORE_GCMODE", mode),
                    );
                }
            }
            jobs.push(
                Job::new(prog, trace)
                    .limits_s(l)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .env("TR_CORE_GCMODE", "gen"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .limits_s(l)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
            );
        }
    }
    jobs
}

#[test]
fn ba_num_matrix() {
    assert!(!compare(ba_jobs("ba_num", &[""])).is_empty());
}

#[test]
fn ba_next_matrix() {
    assert!(!compare(ba_jobs("ba_next", &[""])).is_empty());
}

#[test]
fn ba_find_matrix() {
    assert!(!compare(ba_jobs("ba_find", &["", "libcall=519"])).is_empty());
}

/// BUNDLE-C `tfmt`: `string.format` of constant formats recorded as a `LIB` helper (`tc_fmt`): the program in every trace mode with verification, exit
/// storms, budgets, GC stress 1 and 2 in both collector modes, the stack-move stress and the emulator; the knob alone, with `t1ccall`, and off (the control).
/// Mutants `tfmt_kind_skip`, `tfmt_fmt_guard_skip`, `tfmt_args_swap`.
#[test]
fn tc_fmt_matrix() {
    assert!(!compare(ba_jobs("tc_fmt", &["tfmt=1", "tfmt=1,t1ccall=1", "tfmt=0"])).is_empty());
}

/// BUNDLE-H `libcall`: n-ary `string.char`, `math.random()`, `string.lower` / `upper` / `reverse` / `rep`, `type`, string compares on trace (`bh_lib`): the program
/// in every trace mode with verification, exit storms, budgets, GC stress 1 and 2 in both collector modes, the stack-move stress and the emulator; the
/// recorded calls alone (1), with the lean entries (3), with the inline forms (7), with `rep` (15), and off (the control).
/// Mutants `charn_range_skip`, `charn_order`, `randf_plus_zero`, `randf_scale_off`, `lower_is_upper`, `rep_cap_skip`, `rep_sep_trailing`, `libk_lean_cmp_swapped`,
/// `libk_lean_char_args`, `libk_type_fn_skip`, `libk_type_index_off`.
#[test]
fn bh_lib_matrix() {
    assert!(!compare(ba_jobs(
        "bh_lib",
        &[
            "libcall=1",
            "libcall=7",
            "libcall=15",
            "libcall=31",
            "libcall=0"
        ]
    ))
    .is_empty());
}

/// BUNDLE-R `libcall` bits 5..: `table.unpack` / `table.move` / failing `tonumber` on trace (`br_lib`), the same matrix; bit 5 alone, with the others, off.
/// Mutants `tunpack_meta_skip`, `tunpack_len_guard_skip`, `tunpack_off_by_one`.
#[test]
fn br_lib_matrix() {
    assert!(!compare(ba_jobs(
        "br_lib",
        &[
            "libcall=39",
            "libcall=71",
            "libcall=135",
            "libcall=263",
            "libcall=511",
            "libcall=7"
        ]
    ))
    .is_empty());
}

/// BUNDLE-H: the recorded calls make a loop around `string.char(a, b, c)` / `math.random()` / `s:lower()` install a trace that runs without leaving.
#[test]
fn bh_libcall_changes_the_traces() {
    for (prog, lim) in [
        ("bh_char_hot", "libcall=1"),
        ("bh_rand_hot", "libcall=1"),
        ("bh_case_hot", "libcall=1"),
        ("bh_rep_hot", "libcall=9"),
    ] {
        let mk = |l: &str| Job::new(prog, "always").opts("+verify").limits_s(l);
        let r = run_all(vec![mk(lim), mk("libcall=0")]);
        assert!(r[0].1.ok && r[1].1.ok, "{prog}: {}", r[0].1.stderr);
        assert!(
            r[0].1.insns != r[1].1.insns,
            "{prog}: on {} off {}",
            r[0].1.stats.line(),
            r[1].1.stats.line()
        );
        assert!(
            r[0].1.stats.exits < 300 && r[0].1.stats.entered >= 1 && r[0].1.stats.blacklisted == 0,
            "{prog}: {}",
            r[0].1.stats.line()
        );
    }
}

/// BUNDLE-R (`libcall` bit 4): a `string.rep` whose result class alternates in the loop is a guard on the computed length, which gets a side trace; without
/// the bit the class mismatch is the helper's answer 0 behind the allocation guard (an `Alloc` exit: no side trace, about one exit per two iterations).
/// Mutant `repcls_guard_skip` (the helper's own check then exits as before: caught by the instruction comparison).
#[test]
fn br_rep_class_guard_gets_a_side_trace() {
    let mk = |l: &str| {
        Job::new("bh_rep_mixed_hot", "always")
            .opts("+verify")
            .limits_s(l)
    };
    let r = run_all(vec![mk("libcall=25"), mk("libcall=9")]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.insns < r[1].1.insns,
        "cls {} plain {}",
        r[0].1.stats.line(),
        r[1].1.stats.line()
    );
    assert!(
        r[0].1.stats.entered >= 1 && r[0].1.stats.blacklisted == 0,
        "{}",
        r[0].1.stats.line()
    );
}

/// BUNDLE-H `tnextinl`: `pairs` / `next` loops with the table modified during the traversal (`bh_next`), the same matrix; the knob on and off.
/// Mutant `tnext_hdr_never` (the knob is ignored: caught by `bh_tnext_peels`).
#[test]
fn bh_next_matrix() {
    assert!(!compare(ba_jobs(
        "bh_next",
        &["tnextinl=1", "tnextinl=0", "tnextinl=1,libcall=7"]
    ))
    .is_empty());
}

/// BUNDLE-H: with `tnextinl` the loop around a traversal is peeled (a loop trace), without it it is rolled. Mutant `tnext_hdr_never`.
#[test]
fn bh_tnext_peels() {
    let mk = |l: &str| {
        Job::new("bh_next_hot", "always")
            .opts("+verify")
            .limits_s(l)
    };
    let r = run_all(vec![mk("tnextinl=1"), mk("tnextinl=0")]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.stats.peeled > r[1].1.stats.peeled,
        "peeled on {} off {}",
        r[0].1.stats.line(),
        r[1].1.stats.line()
    );
}

/// BUNDLE-C `t1ccall`: the lean tier-1 call of light C functions (`tc_ccall`), the same matrix (errors through `pcall` / `xpcall`, tracebacks, hooks
/// set during the run, yields across `pcall`, `coroutine.wrap`, stack growth in C calls).
#[test]
fn tc_ccall_matrix() {
    assert!(!compare(ba_jobs(
        "tc_ccall",
        &["t1ccall=1", "t1ccall=1,tfmt=1", "t1ccall=0"]
    ))
    .is_empty());
}

/// BUNDLE-C: the plain tier-1 modes (`t1` = JIT on, traces off, and `off`) with the knobs named (no effect with traces off: the controls) and with
/// the hot / budget variations of `on`.
#[test]
fn tc_t1_modes() {
    let mut jobs = Vec::new();
    for prog in ["tc_fmt", "tc_ccall"] {
        for l in ["t1ccall=1,tfmt=1", "t1ccall=0,tfmt=0"] {
            for trace in ["t1", "off"] {
                jobs.push(Job::new(prog, trace).limits_s(l));
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(l)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .env("TR_CORE_GCMODE", "gen"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(l)
                        .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
                );
            }
        }
    }
    assert!(!compare(jobs).is_empty());
}

/// BUNDLE-C `tfmt`: the recorded call makes the loops of the program install traces that do not leave on every iteration, and without the knob the
/// call stitches (a different instruction count / installs). Mutant `tfmt_fmt_guard_skip` is caught by `tc_fmt_matrix` (the format register changes).
#[test]
fn tc_fmt_knob_changes_the_traces() {
    let mk = |l: &str| Job::new("tc_fmt_hot", "always").opts("+verify").limits_s(l);
    let r = run_all(vec![mk("tfmt=1"), mk("tfmt=0")]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.insns != r[1].1.insns,
        "on {} off {}",
        r[0].1.stats.line(),
        r[1].1.stats.line()
    );
    assert!(
        r[0].1.stats.exits < 300 && r[0].1.stats.entered >= 1 && r[0].1.stats.blacklisted == 0,
        "{}",
        r[0].1.stats.line()
    );
}

/// BUNDLE-A `libnum`: float and integer numerals (hex, `1e2`, `2.5`) in a loop do not leave the trace on every iteration. Mutant `tonum_mode_ignored`
/// (the helper converts like the `for` limit does, floats are "no numeral": the guard exits every time).
#[test]
fn ba_num_does_not_exit_on_numerals() {
    let r = run_all(vec![Job::new("ba_num_floats", "always")
        .opts("+verify")
        .limits_s("")]);
    assert!(r[0].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.stats.exits < 60 && r[0].1.stats.entered >= 1 && r[0].1.stats.blacklisted == 0,
        "{}",
        r[0].1.stats.line()
    );
}

/// BUNDLE-A (K3b: `libnum` / `libnext` / `libfind` are hard-wired on): the recorded library calls make the loops of the programs install and enter traces.
#[test]
fn ba_knobs_change_the_traces() {
    for prog in ["ba_num", "ba_next", "ba_find"] {
        let r = run_all(vec![Job::new(prog, "always").opts("+verify")]);
        assert!(r[0].1.ok, "{prog}: {}", r[0].1.stderr);
        assert!(
            r[0].1.stats.installed >= 1 && r[0].1.stats.entered >= 1,
            "{prog}: {}",
            r[0].1.stats.line()
        );
    }
}

/// HDRSUB: the loop around `string.sub` is peeled with `hdrlib=2` and rolled with `hdrlib=1`. Mutant `hdrlib_sub_always` (peeled at 1).
#[test]
fn hs_sub_peels_only_from_level_2() {
    let mk = |l: &str| {
        Job::new("b8_sub_ring", "always")
            .opts("+verify")
            .limits_s(l)
    };
    let r = run_all(vec![mk("hdrlib=2"), mk("hdrlib=1")]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.stats.peeled > r[1].1.stats.peeled,
        "peeled on {} off {}",
        r[0].1.stats.line(),
        r[1].1.stats.line()
    );
}

/// BUNDLE-1 (4): loops with a string `TLEN` / `string.byte` are peeled with the knob and rolled without it. Mutant `hdrlib_never`.
#[test]
fn b1_hdr_peels_length_loops() {
    let mut on = Job::new("b1_hdr", "always").opts("+verify");
    on.limits = "hdrlib=1".into();
    let mut off = Job::new("b1_hdr", "always").opts("+verify");
    off.limits = "hdrlib=0".into();
    let r = run_all(vec![on, off]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        r[0].1.stats.peeled > r[1].1.stats.peeled,
        "peeled on {} off {}",
        r[0].1.stats.line(),
        r[1].1.stats.line()
    );
}

/// BUNDLE-1 (2): a function reached only by tail calls is compiled with `tailhook` (the `[jit] compiled` count of the run is higher than
/// without it). Mutant `tailhook_ignored`.
#[test]
fn b1_tail_only_callee_is_compiled() {
    let mk = |l: &str| {
        let mut j = Job::new("b1_tail_only", "on")
            .opts("+allblack")
            .env("RUSTYNAIL_JIT_VERBOSE", "1");
        j.limits = l.into();
        j
    };
    let r = run_all(vec![mk("tailhook=1"), mk("tailhook=0")]);
    let compiled = |e: &str| {
        e.lines()
            .find_map(|l| l.strip_prefix("[jit] compiled "))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0)
    };
    let (on, off) = (compiled(&r[0].1.stderr), compiled(&r[1].1.stderr));
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    assert!(
        on > off,
        "compiled with the knob {on}, without {off}: {}",
        r[0].1.stderr
    );
}

/// BUNDLE-1 (1): the work gate refuses early firings but keeps the order in which the loops of a nest form their traces: a gate that re-armed
/// every head to the full threshold made the inner loop's trace the one that ran (the program entered ~15x more traces for the same work).
/// Mutant `wgate_full_rearm`.
#[test]
fn b1_wgate_keeps_the_formation_order() {
    let mk = |l: &str| {
        let mut j = Job::new("b1_nest", "on");
        j.limits = l.into();
        j
    };
    let r = run_all(vec![mk("wgate=0"), mk("wgate=60000"), mk("wgate=250000")]);
    for (_, o) in &r {
        assert!(o.ok, "{}", o.stderr);
    }
    let e0 = r[0].1.stats.entered.max(1);
    eprintln!(
        "b1_nest entered per gate: {:?}",
        r.iter().map(|(_, o)| o.stats.entered).collect::<Vec<_>>()
    );
    for (k, (_, o)) in r.iter().enumerate().skip(1) {
        assert!(
            o.stats.entered <= 2 * e0 + 100,
            "gate {k}: entered {} against {} without a gate: {}",
            o.stats.entered,
            e0,
            o.stats.line()
        );
        // (K3b: the old bound alone no longer detects `wgate_full_rearm` (entries 381901 / 381344 / 378746 without the mutant, 381901 / 361538 / 378108 with it): the entries stay within 2%)
        assert!(
            o.stats.entered * 50 >= e0 * 49,
            "gate {k}: entered {} against {} without a gate (more than 2% fewer): {}",
            o.stats.entered,
            e0,
            o.stats.line()
        );
    }
}

/// BUNDLE-H2: the inline stub of an exit inside inlined frames rebuilds the `CallInfo` nodes itself while no side trace is attached. The program
/// `h2_fmat` shows the rebuilt frames to `debug.getinfo` / `debug.traceback`, results wanted, tail frames and hooks; every mode, with side
/// traces off (every such exit is restored by the stub), exit storms, small budgets and `+verify`. Detector of the
/// `fmat_*` mutants.
#[test]
fn h2_frame_materialise() {
    let mut jobs = Vec::new();
    for trace in ["on16", "always"] {
        for (opts, lim) in [
            ("", ""),
            ("+verify", ""),
            ("", "maxside=0"),
            ("+verify", "maxside=0"),
            ("+exitstorm=3 +verify", "maxside=0"),
            ("+exitstorm=7", ""),
            ("+verify", "fck=1,framemin=0"),
        ] {
            let mut j = Job::new("h2_fmat", trace).opts(opts);
            j.limits = format!("{H1_PROC},{lim}");
            jobs.push(j);
        }
        for b in [1, 17] {
            let mut j = Job::new("h2_fmat", trace).budget(b);
            j.limits = format!("{H1_PROC},maxside=0");
            jobs.push(j);
        }
        let mut j = Job::new("h2_fmat", trace).env("RUSTYNAIL_JIT_STRESS", "stackmove");
        j.limits = format!("{H1_PROC},maxside=0");
        jobs.push(j);
    }
    compare(jobs);
}

// ---- BUNDLE-M ---------------------------------------------------------------------------------------------------------------

/// A scratch directory unique to this call (the tests of a binary run on parallel threads of one process).
fn m_scratch(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "tr_core_m_{tag}_{}_{}",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

/// Hex addresses of a dump differ between two processes; so do pointers stored as 32-bit halves and rip-relative distances of machine code.
fn m_norm_txt(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0' && b.get(i + 1) == Some(&b'x') {
            out.extend_from_slice(b"0xA");
            i += 2;
            while i < b.len() && b[i].is_ascii_hexdigit() {
                i += 1;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// Compares two dump directories: the same traces (`trace-<n>.txt`: IR before / after each pass, snapshots, register allocation,
/// micro-ops; `trace-<n>.bin`: machine code, compared by size only), `Err` names the first difference.
fn m_dumps_equal(a: &std::path::Path, b: &std::path::Path) -> Result<usize, String> {
    let list = |d: &std::path::Path| {
        let mut v: Vec<String> = std::fs::read_dir(d)
            .expect("dump dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("trace-"))
            .collect();
        v.sort();
        v
    };
    let (la, lb) = (list(a), list(b));
    if la != lb {
        return Err(format!("trace files differ: {} vs {}", la.len(), lb.len()));
    }
    for f in &la {
        let (x, y) = (
            std::fs::read(a.join(f)).unwrap(),
            std::fs::read(b.join(f)).unwrap(),
        );
        if f.ends_with(".txt") {
            if m_norm_txt(&x) != m_norm_txt(&y) {
                return Err(format!("{f}: text differs"));
            }
        } else if x.len().abs_diff(y.len()) > 16 + x.len() / 50 {
            // (machine code embeds addresses of the process: the length may differ by an imm8 / imm32 choice, the bytes by the addresses;
            // the instruction-level comparison with those masked is `scripts/m_identity.py`)
            return Err(format!(
                "{f}: machine code size differs: {} / {}",
                x.len(),
                y.len()
            ));
        }
    }
    Ok(la.len())
}

/// BUNDLE-M C1: the recorder's lockstep (`limits.lockstep`) is a cross-check, not an input: with it off every trace of every program is the
/// same IR, the same register allocation and the same micro-ops (the machine code is compared instruction by instruction, addresses masked,
/// by `scripts/m_identity.py`; two runs of one setting differ in its address-derived immediates), the same trace statistics and output. Every
/// program of this corpus in `always` and `on2`, limits `lockstep=1` against `lockstep=0` (the knob is the only difference; a trace that
/// exists on one side only is a difference). The twin programs are deterministic, so a mismatch is a real one; a program that records a
/// different number of traces from run to run is reported as flaky and compared once more. Mutants `nr_img_record_skip`,
/// `light_tnew_unregistered`, `light_charge_skip`.
#[test]
fn m_lockstep_identity() {
    let mut progs: Vec<&'static Prog> = selected(std::env::var_os("TR_CORE_ALL").is_none());
    // (a program that was measured nondeterministic run to run, independent of the lockstep: its trace set depends on timing)
    progs.retain(|p| p.name != "j2_gate_sides");
    let mut jobs = Vec::new();
    let mut dirs = Vec::new();
    for p in &progs {
        for trace in ["always", "on2"] {
            // (`TR_CORE_LS_B=1` compares the knob with itself: the control that shows how much two runs of one setting differ)
            let other: u32 = std::env::var("TR_CORE_LS_B")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            for ls in [1, other] {
                let d = m_scratch(&format!("{}_{trace}_{ls}", p.name));
                // (the string hash seed is random per process: node indices of `HREFK` and the like follow it)
                let mut j = Job::new(p.name, trace)
                    .env("TR_CORE_DUMPDIR", d.to_str().unwrap())
                    .env("TR_CORE_SEED", "0");
                j.limits = format!("lockstep={ls},recnat=1,recany=1");
                jobs.push(j);
                dirs.push(d);
            }
        }
    }
    let res = run_all(jobs);
    let mut bad = Vec::new();
    let mut traces = 0;
    for (k, pair) in res.chunks(2).enumerate() {
        let (on, off) = (&pair[0], &pair[1]);
        let tag = format!("{} [{}]", on.0.prog, on.0.trace);
        if !on.1.ok || !off.1.ok {
            bad.push(format!("{tag}: a child failed"));
            continue;
        }
        if on.1.result != off.1.result {
            bad.push(format!("{tag}: output differs"));
        }
        let (a, b) = (&on.1.stats, &off.1.stats);
        let key = |s: &TStats| {
            (
                s.recorded,
                s.installed,
                s.flushes,
                s.blacklisted,
                s.refused,
                s.side_recorded,
                s.side_installed,
                s.aborts.clone(),
                s.side_aborts.clone(),
            )
        };
        let (da, db) = (&dirs[2 * k], &dirs[2 * k + 1]);
        match m_dumps_equal(da, db) {
            Ok(n) => traces += n,
            Err(e) => bad.push(format!("{tag}: {e}")),
        }
        if key(a) != key(b) {
            bad.push(format!(
                "{tag}: statistics differ: {} / {}",
                a.line(),
                b.line()
            ));
        }
    }
    if std::env::var_os("TR_CORE_KEEP").is_some() {
        eprintln!(
            "[m_lockstep_identity] dumps kept under {}",
            std::env::temp_dir().display()
        );
    } else {
        for d in &dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    assert!(bad.is_empty(), "lockstep on vs off:\n{}", bad.join("\n"));
    assert!(traces > 100, "the corpus produced only {traces} dump files");
}

/// C43 runner survivors: recorded `rawget(t, "str")` (absence proof) and `rawset` (result) in the trace modes, exit storms, budgets and
/// GC stress; traces must run. Detector of `rawget_str_absent_skip`, `rawset_result_nil`.
#[test]
fn rawfn_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for (opts, budget) in [
            ("", 0),
            ("+verify", 0),
            ("+exitstorm=3 +verify", 0),
            ("+exitstorm=7 +verify +randra=5", 0),
            ("", 17),
            ("-loop", 0),
        ] {
            let mut j = Job::new("rawfn_loops", trace).opts(opts);
            if budget != 0 {
                j = j.budget(budget);
            }
            j.limits = String::new();
            jobs.push(j);
        }
        let mut j = Job::new("rawfn_loops", trace).env("RUSTYNAIL_GC_STRESS", "2");
        j.limits = String::new();
        jobs.push(j);
    }
    let res = compare(jobs);
    for (j, o) in &res {
        // (5.4: `rawget` / `rawset` / `next` are not recorded as library calls, `CFunc` aborts the recording: only the output is compared there)
        if j.trace == "on2" && j.opts == "+verify" && !cfg!(rustynail_lua54) {
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.tag(),
                o.stats.line()
            );
        }
    }
}

/// C43: the exit storm must not fail the guard behind the `RANDOM` helper (the generator has advanced; the exit re-executes the `CALL`).
/// Found as an "uvroll miscompile" of `f2_lib`: forwarding upvalue loads only changed how many guards one iteration holds, i.e. which guard
/// the N-th execution hit. Several storm periods (the old failures began at 17), (K3b: `f2*` and `uvsep` hard-wired on). Mutant `rand_storm_guard`.
#[test]
fn random_storm_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for n in [7, 17, 19, 20, 22, 23, 25, 27, 28, 30] {
            for limits in [""] {
                let mut j = Job::new("rand_storm", trace).opts(&format!("+exitstorm={n} +verify"));
                j.limits = limits.into();
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "on2" && j.opts == "+exitstorm=7 +verify" && !j.limits.contains("uvsep") {
            assert!(
                o.stats.installed >= 1 && o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.tag(),
                o.stats.line()
            );
        }
    }
}

/// Debugging aid, not a check of anything (does nothing unless `TR_ADHOC_FILE` is set): runs the Lua file `TR_ADHOC_FILE` as a
/// program of this harness and compares it with the interpreter. `TR_ADHOC_TRACE` (default `always`; `on2`, `on`), `TR_ADHOC_OPTS`
/// (the `+verify +exitstorm=7 ...` option string) and `TR_ADHOC_LIMITS` (a knob string such as `maxside=9,sidemin=2`) select the job; the
/// harness variables (`TR_CORE_UVSEP`, `TR_CORE_PROCESS`, ...) apply as to every job. Used to minimise miscompiles (docs/JIT_PARKED.md C43).
#[test]
fn adhoc_file() {
    let Ok(file) = std::env::var("TR_ADHOC_FILE") else {
        return;
    };
    let var = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
    let mut j = Job::new(&format!("@{file}"), &var("TR_ADHOC_TRACE", "always"))
        .opts(&var("TR_ADHOC_OPTS", ""));
    j.limits = var("TR_ADHOC_LIMITS", "");
    compare(vec![j]);
}

/// BUNDLE-F `stitch0`: a side recording of a function root that would stitch before its first bytecode is refused (it was an empty trace: an entry, an exit
/// at the same `CALL`, hot again, up to 16 deep). Same output as the interpreter in every mode; with the rule on there are fewer side traces and exits than
/// with it off. Mutant `stitch_side_empty`.
#[test]
fn bf_stitch0_programs() {
    let mut jobs = Vec::new();
    for lim in ["stitch0=0", "stitch0=1"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("", 17),
                ("-loop", 0),
            ] {
                let mut j = Job::new("bf_stitch_chain", trace)
                    .opts(opts)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1");
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            jobs.push(
                Job::new("bf_stitch_chain", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
            );
            jobs.push(
                Job::new("bf_stitch_chain", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("RUSTYNAIL_GC_STRESS", "1"),
            );
            jobs.push(
                Job::new("bf_stitch_chain", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("TR_CORE_GCMODE", "inc")
                    .env("RUSTYNAIL_GC_STRESS", "2"),
            );
            jobs.push(
                Job::new("bf_stitch_chain", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("TR_CORE_GCMODE", "gen")
                    .env("RUSTYNAIL_GC_STRESS", "2"),
            );
            jobs.push(
                Job::new("bf_stitch_chain", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .opts("+emurun +verify"),
            );
        }
    }
    jobs.push(Job::new("bf_stitch_chain", "t1"));
    let res = compare(jobs);
    let pick = |lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bf_stitch_chain"
                    && j.trace == "on2"
                    && j.opts.is_empty()
                    && j.budget.is_none()
                    && j.env.len() == 1
                    && j.limits == lim
            })
            .map(|(_, o)| o)
    };
    let (off, on) = (
        pick("stitch0=0").expect("off job"),
        pick("stitch0=1").expect("on job"),
    );
    assert!(
        on.stats.side_installed < off.stats.side_installed,
        "no empty side traces with the rule on: off {} | on {}",
        off.stats.line(),
        on.stats.line()
    );
    assert!(
        on.stats.exits < off.stats.exits,
        "fewer exits with the rule on: off {} | on {}",
        off.stats.line(),
        on.stats.line()
    );
}

/// BUNDLE-F `fibdiet`: the slimmer native-recursion protocol (room check of the new frame from the register that holds its function index). Tree recursion,
/// the recursion to the stack-overflow limit (depth reached and error message), `pcall` around the recursion, a coroutine inside it and a count hook mid-run give
/// the interpreter's output in every mode, with the piece on and off, under the shipped process defaults (`TR_CORE_PROCESS=1`: the native recursion is a
/// process default). Mutant `fd_room_extent_off` (the check admits frames beyond the stack).
#[test]
fn bf_fibdiet_programs() {
    let mut jobs = Vec::new();
    for lim in ["fibdiet=0", "fibdiet=1"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [
                ("", 0),
                ("+verify", 0),
                ("+exitstorm=3 +verify", 0),
                ("", 17),
                ("-loop", 0),
            ] {
                let mut j = Job::new("bf_rec", trace)
                    .opts(opts)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1");
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
            jobs.push(
                Job::new("bf_rec", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
            );
            jobs.push(
                Job::new("bf_rec", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("RUSTYNAIL_GC_STRESS", "1"),
            );
            jobs.push(
                Job::new("bf_rec", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("TR_CORE_GCMODE", "inc")
                    .env("RUSTYNAIL_GC_STRESS", "2"),
            );
            jobs.push(
                Job::new("bf_rec", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .env("TR_CORE_GCMODE", "gen")
                    .env("RUSTYNAIL_GC_STRESS", "2"),
            );
            jobs.push(
                Job::new("bf_rec", trace)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1")
                    .opts("+emurun +verify"),
            );
        }
    }
    jobs.push(Job::new("bf_rec", "t1"));
    // the recursion to the overflow limit: the depth reached and the message equal the interpreter's (a check that is off by a slot moves the depth)
    for lim in ["fibdiet=0", "fibdiet=1"] {
        for trace in ["always", "on2", "on"] {
            for (opts, budget) in [("", 0), ("+verify", 0), ("", 17), ("-loop", 0)] {
                let mut j = Job::new("bf_rec_deep", trace)
                    .opts(opts)
                    .limits_s(lim)
                    .env("TR_CORE_PROCESS", "1");
                if budget != 0 {
                    j = j.budget(budget);
                }
                jobs.push(j);
            }
        }
    }
    jobs.push(Job::new("bf_rec_deep", "t1"));
    compare(jobs);
}

/// BUNDLE-J (`centry`): Lua functions called from C (sort comparators, gsub callbacks, pcall'd functions) enter compiled code as C-entered function roots.
/// Every program computes what the interpreter computes with `centry=0/1/2` and `centrymin=0` (no weight rule) in both trace modes, with the lockstep,
/// exit storms, a small budget, GC stress and the emulator; the sort programs install a C-entered root.
#[test]
fn centry_programs_run_on_traces() {
    let mut jobs = Vec::new();
    for p in [
        "j1_sort_objects",
        "j1_callbacks",
        "j1_errors",
        "j1_gc_hooks",
    ] {
        for trace in ["always", "on2"] {
            for l in [
                "centry=2,centrymin=0",
                "centry=1,centrymin=0",
                "centry=2",
                "centry=0",
            ] {
                jobs.push(Job::new(p, trace).limits_s(l).opts("+verify"));
            }
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .opts("+exitstorm=3 +verify"),
            );
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .opts("+exitstorm=7 +verify +randra=5"),
            );
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .budget(17),
            );
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .opts("+emurun +verify"),
            );
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .env("RUSTYNAIL_GC_STRESS", "2"),
            );
            jobs.push(
                Job::new(p, trace)
                    .limits_s("centry=2,centrymin=0")
                    .env("RUSTYNAIL_GC_STRESS", "1"),
            );
        }
        jobs.push(Job::new(p, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.prog == "j1_sort_objects"
            && j.trace == "always"
            && j.limits == "centry=2,centrymin=0"
            && j.opts == "+verify"
        {
            assert!(
                o.stats.installed >= 1,
                "{}: no trace installed: {}",
                j.prog,
                o.stats.line()
            );
        }
    }
}

/// Detector of the `centry` mutants `centry_fnmin_kept`, `centry_weight_ignored`, `centry_no_head`, `centry_not_promoted`: the smallest comparator (`a > b`: a handful
/// of bytecodes, no `CALL`) is a C-entered function root with `centrymin=0` (a trace is installed and entered for the calls from `table.sort`), and with the default
/// weight rule it is too light to repay the entry from C (the traces entered stay a small fraction of the calls).
#[test]
fn centry_small_comparator_is_a_root() {
    let res = compare(vec![
        Job::new("j1_small_cmp", "always")
            .limits_s("centry=2,centrymin=0")
            .opts("+verify"),
        Job::new("j1_small_cmp", "always")
            .limits_s("centry=2")
            .opts("+verify"),
        Job::new("j1_small_cmp", "always")
            .limits_s("centry=0")
            .opts("+verify"),
        Job::new("j1_small_cmp", "on2").limits_s("centry=2,centrymin=0"),
    ]);
    for (j, o) in &res {
        match (j.trace.as_str(), j.limits.as_str()) {
            ("always", "centry=2,centrymin=0") | ("on2", "centry=2,centrymin=0") => {
                assert!(
                    o.stats.installed >= 1,
                    "{} {}: no C-entered root installed: {}",
                    j.prog,
                    j.limits,
                    o.stats.line()
                );
                assert!(
                    o.stats.entered >= 1000,
                    "{} {}: the calls from C do not enter the root: {}",
                    j.prog,
                    j.limits,
                    o.stats.line()
                );
            }
            ("always", "centry=2") => {
                assert!(
                    o.stats.entered < 500,
                    "{}: the light root is entered from C (weight rule): {}",
                    j.prog,
                    o.stats.line()
                );
            }
            ("always", "centry=0") => {
                assert!(
                    o.stats.entered < 500,
                    "{}: entered from C with centry=0: {}",
                    j.prog,
                    o.stats.line()
                );
            }
            _ => {}
        }
    }
}

/// Detector of the `centry` mutant `centry_black_keeps_entering`: the C-entered head of a comparator that cannot be traced goes `Black`, and the calls from C then
/// stay in the interpreter (the entries stay a small fraction of the calls: 40 sorts of 60 elements make over 10000 comparator calls).
#[test]
fn centry_blacklisted_comparator_leaves_c_entry() {
    let res = compare(vec![
        Job::new("j1_loop_cmp", "always")
            .limits_s("centry=2,centrymin=0")
            .opts("+verify"),
        Job::new("j1_loop_cmp", "on2").limits_s("centry=2,centrymin=0"),
        Job::new("j1_loop_cmp", "always").limits_s("centry=0"),
    ]);
    for (j, o) in &res {
        if j.limits.starts_with("centry=2") {
            assert!(
                o.stats.c_registered >= 1,
                "{} {}: no C-entered prototype registered: {}",
                j.prog,
                j.trace,
                o.stats.line()
            );
            assert!(
                o.stats.blacklisted >= 1,
                "{} {}: the comparator head was not blacklisted: {}",
                j.prog,
                j.trace,
                o.stats.line()
            );
            assert!(
                o.stats.c_entries < 1500,
                "{} {}: {} calls from C entered compiled code after the head went black: {}",
                j.prog,
                j.trace,
                o.stats.c_entries,
                o.stats.line()
            );
        }
    }
}

/// Detector of the `centry` mutant `centry_wait_ignored`: a registered C-entered function is entered only from its `centrywait`-th hot event, so with a long wait the root
/// is entered for fewer of the calls than with none.
#[test]
fn centry_registration_waits_for_hot_calls() {
    let res = compare(vec![
        Job::new("j1_small_cmp", "always").limits_s("centry=2,centrymin=0,centrywait=0"),
        Job::new("j1_small_cmp", "always").limits_s("centry=2,centrymin=0,centrywait=400"),
    ]);
    let ent = |w: &str| {
        res.iter()
            .find(|(j, _)| j.limits.ends_with(w))
            .map(|(_, o)| o.stats.entered)
            .unwrap()
    };
    assert!(
        ent("centrywait=0") >= ent("centrywait=400") + 300,
        "a long wait does not delay the registration: {} vs {}",
        ent("centrywait=0"),
        ent("centrywait=400")
    );
}

/// BUNDLE-P P2 `f2ifl`: the `F2I` guard behind a `FFLOOR` is `cvttsd2si; cmp r, 1; jo`. Floor / ceil of floats at the edges of the integer range (NaN, +-inf, +-2^63,
/// -2^63 - 1024, 0.5, -0.5, huge values, integral floats) and of exactly -2^63 (the one conservative exit) give the interpreter's output in every trace mode with and
/// without the knob, plain and with `+verify`, the emulator, exit storms and budgets. `bp_floor_minint` exits at the guard with the knob (never without it).
/// Mutants `f2ifl_all` (the cheap guard also behind other conversions: `bp_floor` indexes with non-integral floats), `f2ifl_never` (the exit counts of
/// `bp_floor_minint` do not differ).
#[test]
fn bp_f2ifl_programs() {
    const LIMS: [&str; 2] = ["f2ifl=0", "f2ifl=1"];
    let mut jobs = Vec::new();
    for prog in ["bp_floor", "bp_floor_minint"] {
        for trace in ["always", "on2", "on"] {
            for lim in LIMS {
                for (opts, budget) in [
                    ("", 0),
                    ("+verify", 0),
                    ("+exitstorm=3 +verify", 0),
                    ("", 17),
                    ("-loop", 0),
                ] {
                    let mut j = Job::new(prog, trace).opts(opts).limits_s(lim);
                    if budget != 0 {
                        j = j.budget(budget);
                    }
                    jobs.push(j);
                }
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(Job::new(prog, trace).limits_s(lim).opts("+emurun +verify"));
            }
        }
        jobs.push(Job::new(prog, "t1"));
    }
    let res = compare(jobs);
    for (j, o) in &res {
        if j.trace == "always" && j.opts == "+verify" && j.env.is_empty() {
            assert!(
                o.stats.entered >= 1,
                "{}: traces did not run: {}",
                j.tag(),
                o.stats.line()
            );
        }
    }
    // the conservative exit: the floor of exactly -2^63 leaves the trace with the knob, and only with it
    let find = |lim: &str| {
        res.iter()
            .find(|(k, _)| {
                k.prog == "bp_floor_minint"
                    && k.trace == "always"
                    && k.opts == "+verify"
                    && k.limits == lim
                    && k.env.is_empty()
            })
            .map(|(_, o)| o.stats.exits)
            .expect("job")
    };
    let (off, on) = (find("f2ifl=0"), find("f2ifl=1"));
    assert!(on > off, "f2ifl=1 exits {on} not above f2ifl=0 exits {off}");
}

/// BUNDLE-Q `t1q`: tier-1 `EQ` of objects (bit 0: same payload, both metatables absent / flagged without `__eq`, `jit_cmp` for the rest) and `MOD` / `MODK` with a float
/// operand (bit 1: `jit_fmod`). `bq_eq` (same table, distinct tables with and without metatables, `__eq` returning true / false / 1 / nil / "x", `__eq` on one operand
/// only (both orders), `__eq` added / removed / replaced after the absence was cached, `setmetatable` changing the answer, erroring and yielding `__eq`, functions,
/// coroutines, userdata (io files), long strings, NaN, numbers) and `bq_mod` (every sign combination, zeros, `-0.0`, inf, NaN, huge values, integer / float mixes, a
/// zero divisor of both kinds, `minint % -1`) give the interpreter's output in every trace mode and in tier 1 alone, with the bits 0 / 1 / 2 / 3, plain and `+verify`,
/// exit storms, budgets, GC stress, stack moves and the emulator. In tier 1 alone compiled code executes more bytecodes (fewer exits) with the bit than without it.
/// Mutants `t1q_eq_same_false`, `t1q_eq_nomm_true`, `t1q_eq_second_mm_skip` (`bq_eq`), `t1q_mod_sign_skip`, `t1q_mod_negzero`, `t1q_mod_inf_fix` (`bq_mod`).
#[test]
fn bq_t1q_programs() {
    let mut jobs = Vec::new();
    for prog in ["bq_eq", "bq_mod"] {
        for trace in ["t1", "always", "on2", "on"] {
            for lim in ["t1q=0", "t1q=1", "t1q=2", "t1q=3"] {
                jobs.push(Job::new(prog, trace).limits_s(lim));
                if trace != "t1" {
                    jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                }
            }
            if trace != "t1" {
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s("t1q=3"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+emurun +verify")
                        .limits_s("t1q=3"),
                );
                jobs.push(Job::new(prog, trace).budget(17).limits_s("t1q=3"));
            }
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("t1q=3"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s("t1q=3"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove")
                    .limits_s("t1q=3"),
            );
        }
        jobs.push(Job::new(prog, "t1"));
        jobs.push(Job::new(prog, "on"));
    }
    let res = compare(jobs);
    let find = |prog: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == prog
                    && j.trace == "t1"
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| o.insns)
            .expect("job")
    };
    // (`Out::insns` = bytecodes executed by compiled code: the answered cases and the float modulo no longer leave tier 1 to the interpreter)
    for (prog, bit) in [("bq_eq", "t1q=1"), ("bq_mod", "t1q=2")] {
        let (off, on) = (find(prog, "t1q=0"), find(prog, bit));
        eprintln!("{prog} [t1] compiled-code instructions: t1q=0 {off}, {bit} {on}");
        assert!(
            on > off,
            "{prog}: {bit} runs {on} compiled-code instructions, t1q=0 runs {off}"
        );
    }
    assert!(
        find("bq_eq", "t1q=3") >= find("bq_eq", "t1q=1")
            && find("bq_mod", "t1q=3") >= find("bq_mod", "t1q=2")
    );
}

/// BUNDLE-S S2 `amt`: array reads / stores of tables that have a metatable are recorded (a non-nil element: raw access; a nil element: the flag guard of `__index` /
/// `__newindex`). `bs_amt` (no `__index`, Lua and table `__index`, holes punched and refilled mid-loop, `__index` added / removed / replaced after the absence
/// was cached, `setmetatable(t, nil)`, mixed tables in one loop, zero / negative / huge / float / string keys, stores over existing and nil slots with a `__newindex`
/// that is added / removed / replaced, erroring, redirecting and rawset-ing `__newindex`, a yielding `__index` in a coroutine) gives the interpreter's output in every trace
/// mode, with `amt` 0 and 1, plain and `+verify`, exit storms, budgets, GC stress, stack moves and the emulator; with `amt=1` the `MetaTable` aborts of the array
/// accesses are gone (fewer than with `amt=0`). Mutants `amt_get_nil_skip`, `amt_set_nil_skip`, `amt_set_old_skip`.
#[test]
fn bs_amt_programs() {
    let mut jobs = Vec::new();
    for prog in ["bs_amt"] {
        for trace in ["always", "on2", "on"] {
            for lim in ["amt=0", "amt=1"] {
                for (opts, budget) in [
                    ("", 0),
                    ("+verify", 0),
                    ("+exitstorm=3 +verify", 0),
                    ("", 17),
                    ("-loop +verify", 0),
                ] {
                    let mut j = Job::new(prog, trace).opts(opts).limits_s(lim);
                    if budget != 0 {
                        j = j.budget(budget);
                    }
                    jobs.push(j);
                }
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_JIT_STRESS", "stackmove"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "2"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .limits_s(lim)
                        .env("RUSTYNAIL_GC_STRESS", "1"),
                );
                jobs.push(Job::new(prog, trace).limits_s(lim).opts("+emurun +verify"));
            }
        }
        jobs.push(Job::new(prog, "t1"));
        jobs.push(Job::new(prog, "on"));
    }
    let res = compare(jobs);
    let find = |lim: &str| {
        res.iter()
            .find(|(k, _)| {
                k.prog == "bs_amt"
                    && k.trace == "always"
                    && k.opts == "+verify"
                    && k.limits == lim
                    && k.env.is_empty()
            })
            .map(|(_, o)| o.stats.aborts.get("MetaTable").copied().unwrap_or(0))
            .expect("job")
    };
    let (off, on) = (find("amt=0"), find("amt=1"));
    eprintln!("bs_amt MetaTable aborts: amt=0 {off}, amt=1 {on}");
    assert!(
        off > on,
        "amt=1 MetaTable aborts {on} not below amt=0 aborts {off}"
    );
}

/// BUNDLE-S S1 `t1q` bits 2 / 3: tier-1 table reads whose `__index` is a Lua function and stores whose `__newindex` is a Lua function run the handler as an in-place frame
/// of compiled code (`jit_get` / `jit_set` push it, `jit_mm_ret` finishes the instruction). `bs_ixmm` (getfield / gettable / geti / self / gettabup through function
/// `__index`, chains of tables ending in a function, handlers returning none / many values, erroring, yielding, tracebacks and `debug` calls from the handler, a hook
/// installed from the handler, the metatable / the handler / the table mutated mid-loop, runaway recursion, strings / numbers / userdata not affected, odd keys, and the
/// same for `__newindex` in all four store instructions) gives the interpreter's output in every trace mode and in tier 1 alone, with the bits 0 / 4 / 8 / 12 / 15, plain
/// and `+verify`, exit storms, budgets, GC stress, stack moves and the emulator. In tier 1 alone compiled code executes more bytecodes (fewer exits) with the bits.
/// Mutants `s1_self_skip`, `s1_get_dst_off`, `s1_savedpc_skip`, `s1_count_skip`, `s1_pay_skip`, `s1_land_skip`, `s1_obj_orig`.
#[test]
fn bs_ixmm_programs() {
    let mut jobs = Vec::new();
    for prog in ["bs_ixmm"] {
        for trace in ["t1", "always", "on2", "on"] {
            for lim in ["t1q=1", "t1q=5", "t1q=9", "t1q=13", "t1q=15"] {
                jobs.push(Job::new(prog, trace).limits_s(lim));
                if trace != "t1" {
                    jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                }
            }
            if trace != "t1" {
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s("t1q=13"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+emurun +verify")
                        .limits_s("t1q=13"),
                );
            }
            jobs.push(Job::new(prog, trace).budget(17).limits_s("t1q=13"));
            jobs.push(Job::new(prog, trace).budget(1000).limits_s("t1q=13"));
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("t1q=13"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s("t1q=13"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove")
                    .limits_s("t1q=13"),
            );
        }
        jobs.push(Job::new(prog, "t1"));
        jobs.push(Job::new(prog, "on"));
    }
    // (with `t15_oracle` the instruction count of the compiled run equals the interpreter's: the pre-paid counts of the block are given back and paid again)
    #[cfg(feature = "t15_oracle")]
    for prog in ["bs_ixmm"] {
        for b in [None, Some(3), Some(17), Some(200)] {
            for trace in ["always", "on2"] {
                let mut j = Job::new(prog, trace).limits_s("t1q=13");
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    let find = |lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bs_ixmm"
                    && j.trace == "t1"
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| o.insns)
            .expect("job")
    };
    // (`Out::insns` = bytecodes executed by compiled code: the metamethod frames no longer leave tier 1)
    let base = find("t1q=1");
    for lim in ["t1q=5", "t1q=9", "t1q=13"] {
        let on = find(lim);
        eprintln!("bs_ixmm [t1] compiled-code instructions: t1q=1 {base}, {lim} {on}");
        assert!(
            on > base,
            "{lim} runs {on} compiled-code instructions, t1q=1 runs {base}"
        );
    }
}

/// BUNDLE-V V1 `t1q` bits 4 / 5: tier-1 arithmetic / bitwise / unary metamethods (`Cold::ArithMm`) run the Lua handler as an in-place frame with private (flagged) continuation
/// entries (bit 4), and `TAILCALL` / the general `RETURN` (several or open results, vararg frames) inside a metamethod frame stay in compiled code (bit 5; a native tail callee
/// finishes the caller's instruction through `jit_mm_pop`). `bv_mm` (every arithmetic and bitwise operator with a table on either side and with constants, handlers that tail
/// call a Lua function / a vararg function / natives / `__call` tables and chains, handlers returning 0 / 1 / many / odd values, erroring handlers with tracebacks, `debug` calls and
/// hooks from handlers, yields (also through a tail-called `coroutine.yield`), metatables mutated by the handler, deep and runaway recursion, `__lt` / `__le` / `__eq` / `__len` / `__concat`
/// handlers with tail calls, the vararg `__call` constructor, closures created by handlers) gives the interpreter's output in every trace mode and in tier 1 alone, with the bits
/// 0 / 4 / 5 / 4+5 on top of 13, plain and `+verify`, exit storms, budgets, GC stress, stack moves and the emulator. Mutants: see `tr::mutants` (`bv_*`).
#[test]
fn bv_mm_programs() {
    let mut jobs = Vec::new();
    for prog in ["bv_mm"] {
        for trace in ["t1", "always", "on2", "on"] {
            for lim in [
                "t1q=13", "t1q=29", "t1q=45", "t1q=77", "t1q=125", "t1q=112", "t1q=1",
            ] {
                jobs.push(Job::new(prog, trace).limits_s(lim));
                if trace != "t1" {
                    jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
                }
            }
            if trace != "t1" {
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+exitstorm=3 +verify")
                        .limits_s("t1q=125"),
                );
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+emurun +verify")
                        .limits_s("t1q=125"),
                );
            }
            jobs.push(Job::new(prog, trace).budget(17).limits_s("t1q=125"));
            jobs.push(Job::new(prog, trace).budget(1000).limits_s("t1q=125"));
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("t1q=125"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s("t1q=125"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove")
                    .limits_s("t1q=125"),
            );
        }
        jobs.push(Job::new(prog, "t1"));
        jobs.push(Job::new(prog, "on"));
    }
    #[cfg(feature = "t15_oracle")]
    for prog in ["bv_mm"] {
        for b in [None, Some(3), Some(17), Some(200)] {
            for trace in ["always", "on2"] {
                let mut j = Job::new(prog, trace).limits_s("t1q=125");
                j.budget = b;
                j.ident = true;
                jobs.push(j);
            }
        }
    }
    let res = compare(jobs);
    let find = |lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bv_mm"
                    && j.trace == "t1"
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| o.insns)
            .expect("job")
    };
    // (`Out::insns` = bytecodes executed by compiled code: the metamethod frames no longer leave tier 1)
    let base = find("t1q=13");
    for lim in ["t1q=29", "t1q=125"] {
        let on = find(lim);
        eprintln!("bv_mm [t1] compiled-code instructions: t1q=13 {base}, {lim} {on}");
        assert!(
            on > base,
            "{lim} runs {on} compiled-code instructions, t1q=13 runs {base}"
        );
    }
}

/// BUNDLE-V V2 `upbar`: `SETUPVAL` of a collectable value into a real upvalue cell is recorded as `USTORE` + `TBAR` (aux 1: the forward barrier on the `UpVal`), instead
/// of aborting `Nyi(Setupval)`. `bv_upbar` (LRU list with head / tail upvalues, stored kinds, an upvalue that aliases a register of the loop's frame, open upvalues
/// shared between coroutines, closed upvalues, stores from metamethods; `collectgarbage("step")` in the loops) gives the interpreter's output in every trace mode with
/// `upbar` 0 / 1, plain and `+verify`, exit storms, GC stress 1 / 2 in both collectors (`GC_VERIFY`), stack moves and the emulator. Mutant `upbar_skip` (no barrier).
#[test]
fn bv_upbar_programs() {
    let mut jobs = Vec::new();
    for prog in ["bv_upbar"] {
        for trace in ["always", "on2", "on"] {
            for lim in ["upbar=0", "upbar=1"] {
                jobs.push(Job::new(prog, trace).limits_s(lim));
                jobs.push(Job::new(prog, trace).opts("+verify").limits_s(lim));
            }
            jobs.push(
                Job::new(prog, trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .opts("+emurun +verify")
                    .limits_s("upbar=1"),
            );
            jobs.push(Job::new(prog, trace).budget(17).limits_s("upbar=1"));
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s("upbar=1"),
            );
            // (`Heap::verify` after every step: a black upvalue that holds a white value, the missing barrier, is reported at once)
            jobs.push(
                Job::new(prog, trace)
                    .env("TR_CORE_GCMODE", "inc")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("TR_CORE_GCMODE", "gen")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("TR_CORE_GCMODE", "inc")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("TR_CORE_GCMODE", "gen")
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove")
                    .limits_s("upbar=1"),
            );
        }
        jobs.push(Job::new(prog, "t1"));
        jobs.push(Job::new(prog, "on"));
    }
    let res = compare(jobs);
    let setupval_aborts = |lim: &str| -> usize {
        res.iter()
            .find(|(j, _)| {
                j.prog == "bv_upbar"
                    && j.trace == "always"
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| {
                o.stats
                    .aborts
                    .iter()
                    .filter(|(k, _)| k.starts_with("Nyi"))
                    .map(|(_, n)| *n as usize)
                    .sum()
            })
            .expect("job")
    };
    eprintln!(
        "bv_upbar Nyi(Setupval) aborts: upbar=0 {}, upbar=1 {}",
        setupval_aborts("upbar=0"),
        setupval_aborts("upbar=1")
    );
}

/// Y5: the forward barrier of an upvalue store (`upbar`: `USTORE` + `TBAR` aux 1, helper `jit_tr_barrier` with `b` = 1) in real runs. The inline test calls the
/// helper only for a black upvalue and a white value, a case the hand-built asm test (`tr_asm::hand_upvalue_tbar_calls_the_barrier_exactly_for_black_upvalues_and_white_values`)
/// proves in isolation and `bv_upbar` reaches only a handful of times. `y5_upbar_black`: a 40000-table heap (the incremental mark phase spans many steps; the
/// generational cell and closure become old) and a loop that stores a fresh table into two closed cells, in both collector modes: the child's `[trace] barrier
/// helper calls` line must show upvalue barrier calls (`barrier_upvalue` > 0), and the output must equal the interpreter's. `y5_upbar_black_small` runs under
/// GC stress with `Heap::verify` after every step (a white table referenced from a black cell is reported at once). Mutants `upbar_skip`, `upbar_asm_skip`.
#[test]
fn y5_upbar_black_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2"] {
        for gm in ["inc", "gen"] {
            jobs.push(
                Job::new("y5_upbar_black", trace)
                    .env("TR_CORE_GCMODE", gm)
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new("y5_upbar_black", trace)
                    .env("TR_CORE_GCMODE", gm)
                    .opts("+verify")
                    .limits_s("upbar=1"),
            );
            jobs.push(
                Job::new("y5_upbar_black", trace)
                    .env("TR_CORE_GCMODE", gm)
                    .limits_s("upbar=0"),
            );
            for stress in ["1", "2"] {
                jobs.push(
                    Job::new("y5_upbar_black_small", trace)
                        .env("TR_CORE_GCMODE", gm)
                        .env("RUSTYNAIL_GC_STRESS", stress)
                        .env("RUSTYNAIL_GC_VERIFY", "1")
                        .limits_s("upbar=1"),
                );
            }
            jobs.push(
                Job::new("y5_upbar_black_small", trace)
                    .env("TR_CORE_GCMODE", gm)
                    .env("RUSTYNAIL_GC_VERIFY", "1")
                    .limits_s("upbar=1"),
            );
        }
        jobs.push(
            Job::new("y5_upbar_black", trace)
                .budget(17)
                .limits_s("upbar=1"),
        );
        jobs.push(
            Job::new("y5_upbar_black", trace)
                .opts("+exitstorm=3 +verify")
                .limits_s("upbar=1"),
        );
    }
    jobs.push(Job::new("y5_upbar_black", "t1"));
    let res = compare(jobs);
    for gm in ["inc", "gen"] {
        let o = res
            .iter()
            .find(|(j, _)| {
                j.prog == "y5_upbar_black"
                    && j.trace == "always"
                    && j.limits == "upbar=1"
                    && j.opts.is_empty()
                    && j.budget.is_none()
                    && j.env.iter().any(|(k, v)| k == "TR_CORE_GCMODE" && v == gm)
            })
            .map(|(_, o)| o)
            .expect("job");
        eprintln!(
            "y5_upbar_black [{gm}] barrier helper calls: table {} upvalue {}",
            o.stats.barrier_table, o.stats.barrier_upvalue
        );
        assert!(o.stats.barrier_upvalue > 0, "y5_upbar_black [{gm}]: no real run reached the upvalue barrier (black upvalue, white value): {}", o.stats.line());
    }
}

/// Y2: the exit storm does not fail the guard behind a `TSET` (`table.insert` / `t[#t + 1] = v` in a trace): found by `tr_oracle::oracle_exits` on
/// `bench/programs/table_array` with `+exitstorm=7 +randra=5` (a spurious exit after the store resumed at the `CALL`: the element was stored twice).
/// `y2_insert_storm` gives the interpreter's output in every trace mode under storms of several periods and RA seeds. Test-only machinery (a real
/// failure of that guard means nothing was stored); the exemption is `asm::guard_label`'s `alloc_guard`.
#[test]
fn y2_tset_storm_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for opts in [
            "+exitstorm=7 +randra=5",
            "+exitstorm=7",
            "+exitstorm=3 +verify",
            "+exitstorm=5 +randra=5 +verify",
            "+exitstorm=11 +randra=5",
            "+exitstorm=2 +randra=9 -loop",
        ] {
            jobs.push(Job::new("y2_insert_storm", trace).opts(opts));
        }
    }
    compare(jobs);
}

/// BUNDLE-VA: `table.move` of a range of the same table (overlapping, copy order forward / backward) is unrolled in the trace as interleaved element
/// loads and stores. A guard that fails between two stores (a later source element changes type: natural, no storm needed) exits to the `CALL`, which the
/// interpreter re-runs from the start over the half-moved elements: the result differs unless the loads (and their guards) all come before the first
/// store. `va_tmove_overlap` gives the interpreter's output in the trace modes, plain and under storms (`br_lib` `+exitstorm=7` found it).
/// Mutant `tmove_no_preread`.
#[test]
fn va_tmove_overlap_programs() {
    let mut jobs = Vec::new();
    for trace in ["always", "on2", "on"] {
        for opts in [
            "",
            "+verify",
            "-fwd -cse",
            "+exitstorm=7",
            "-fold -cse +exitstorm=7",
            "+exitstorm=3 +verify",
        ] {
            jobs.push(Job::new("va_tmove_overlap", trace).opts(opts));
        }
    }
    compare(jobs);
}

/// BUNDLE-P P1 / C45 `rearmside` + `rearmclean`: a rearmed tick enters a young head (fewer than `REARM_MIN_ENTRIES` entries of its root trace) when its family already has
/// `rearmside` installed side traces and (`rearmclean=1`) the family is clean: no failed side recording, no unrolled callee loop. `bp_rearm_young_clean` (a loop with a four-class
/// dispatch: one side trace per class; the family never fails a side recording) gives the interpreter's output in every trace mode, with the knob at 0 / 1 / 2 / 64, clean
/// test on and off, plain and `+verify`, exit storms, budgets, GC stress and the emulator; with the production tick period (`on`) the knob makes the rearmed tick enter
/// the head (`rearm_enters` above the R1 run's). `bp_rearm_young_dirty` is the same loop whose family had a failed side recording: with the clean test on it is entered
/// exactly as often as with `rearmside=0`, with it off (`rearmclean=0`) it is entered (so the test is what keeps it out). Mutants `rearmside_never`, `rearmside_any`,
/// `rearmclean_ignores_faults`, `rearmclean_ignores_la` (`bp_rearm_young_la`), `rearmclean_default_ignored` (the rule itself: `tr_policy::bp_rearmside_rule`).
#[test]
fn bp_rearmside_programs() {
    let mut jobs = Vec::new();
    // the `while` loop of `bp_rearm_young_la` is only unrolled into the caller's trace (not traced as a root of its own first) behind the start gate of a process (`wgate`)
    let lim_of = |prog: &str, l: &str| {
        if prog == "bp_rearm_young_la" {
            format!("{l},wgate=30000")
        } else {
            l.to_string()
        }
    };
    for prog in [
        "bp_rearm_young_clean",
        "bp_rearm_young_dirty",
        "bp_rearm_young_la",
    ] {
        for trace in ["always", "on2", "on"] {
            for lim in [
                "rearmside=0",
                "rearmside=1",
                "rearmside=2",
                "rearmside=2,rearmclean=0",
                "rearmside=64",
            ] {
                jobs.push(
                    Job::new(prog, trace)
                        .opts("+verify")
                        .limits_s(&lim_of(prog, lim)),
                );
                jobs.push(Job::new(prog, trace).limits_s(&lim_of(prog, lim)));
            }
            jobs.push(
                Job::new(prog, trace)
                    .opts("+exitstorm=3 +verify")
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .opts("+exitstorm=7 +verify +randra=5")
                    .limits_s(&lim_of(prog, "rearmside=1,rearmclean=0")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .budget(17)
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .opts("+emurun +verify")
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "2")
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_GC_STRESS", "1")
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
            jobs.push(
                Job::new(prog, trace)
                    .env("RUSTYNAIL_JIT_STRESS", "stackmove")
                    .limits_s(&lim_of(prog, "rearmside=2")),
            );
        }
        jobs.push(Job::new(prog, "t1"));
    }
    let res = compare(jobs);
    let find = |prog: &str, lim: &str| {
        let lim = lim_of(prog, lim);
        res.iter()
            .find(|(j, _)| {
                j.prog == prog
                    && j.trace == "on"
                    && j.limits == lim
                    && j.opts.is_empty()
                    && j.env.is_empty()
                    && j.budget.is_none()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    // the clean twin: the production default (rearmside=2, rearmclean=1) enters the young head
    let (off, on, plain) = (
        find("bp_rearm_young_clean", "rearmside=0"),
        find("bp_rearm_young_clean", "rearmside=2"),
        find("bp_rearm_young_clean", "rearmside=2,rearmclean=0"),
    );
    eprintln!("bp_rearm_young_clean [on] rearmside=0: armed {} entered {} | rearmside=2: armed {} entered {} | rearmclean=0: entered {}", off.rearm_arms, off.rearm_enters, on.rearm_arms, on.rearm_enters, plain.rearm_enters);
    assert!(
        on.rearm_enters > off.rearm_enters,
        "rearmside=2 enters the clean young head: {} vs {}",
        on.rearm_enters,
        off.rearm_enters
    );
    assert_eq!(
        on.side_failed, 0,
        "the clean twin has no failed side recording"
    );
    assert_eq!(on.rearm_unclean, (0, 0), "the clean twin is never refused");
    // the faulty twin: kept out by the clean test, entered without it
    let (off, on, plain) = (
        find("bp_rearm_young_dirty", "rearmside=0"),
        find("bp_rearm_young_dirty", "rearmside=2"),
        find("bp_rearm_young_dirty", "rearmside=2,rearmclean=0"),
    );
    eprintln!("bp_rearm_young_dirty [on] rearmside=0: entered {} | rearmside=2: entered {} | rearmclean=0: entered {} (failed side recordings {})", off.rearm_enters, on.rearm_enters, plain.rearm_enters, on.side_failed);
    assert!(
        on.side_failed > 0,
        "the faulty twin has a failed side recording"
    );
    assert_eq!(
        on.rearm_enters, off.rearm_enters,
        "a family with a failed side recording is not entered young: {} vs {}",
        on.rearm_enters, off.rearm_enters
    );
    assert!(
        plain.rearm_enters > off.rearm_enters,
        "without the clean test the faulty family is entered: {} vs {}",
        plain.rearm_enters,
        off.rearm_enters
    );
    assert!(
        on.rearm_unclean.0 > 0,
        "the refusals are counted: {:?}",
        on.rearm_unclean
    );
    assert_eq!(plain.rearm_unclean, (0, 0), "rearmclean=0 refuses nothing");
    // the loopany twin: refused for the unrolled callee loop (mutant `rearmclean_ignores_la`)
    let (on, plain) = (
        find("bp_rearm_young_la", "rearmside=2"),
        find("bp_rearm_young_la", "rearmside=2,rearmclean=0"),
    );
    eprintln!(
        "bp_rearm_young_la [on] rearmside=2: entered {} unclean {:?} | rearmclean=0: entered {}",
        on.rearm_enters, on.rearm_unclean, plain.rearm_enters
    );
    assert!(
        on.rearm_unclean.1 > 0,
        "a family that unrolled a callee loop is refused: {:?}",
        on.rearm_unclean
    );
    assert_eq!(plain.rearm_unclean, (0, 0), "rearmclean=0 refuses nothing");
}

// ---- BUNDLE-N: RETF (docs/JIT_PARKED.md C38) ------------------------------------------------------------------------------------------------

const NR_PROGNAMES: &[&str] = &[
    "n_retf_basic",
    "n_retf_tail",
    "n_retf_multret",
    "n_retf_ccaller",
    "n_retf_two_sites",
    "n_retf_mm",
    "n_retf_getlocal",
    "n_retf_hook",
    "n_retf_vararg_caller",
    "n_retf_closeret",
    "n_retf_coro",
    "n_retf_deep",
    "n_retf_gc",
    "n_retf_results",
    "n_retf_upvalue_alias",
    "n_retf_mr_hot",
    "n_retf_continue",
    "n_retf_untraced",
    "n_retf_relink",
    "n_retf_recursion",
    "n_retf_two_levels",
    "n_retf_after_exits",
    "n_retf_types",
    "n_retf_upvals",
    "n_retf_poly",
    "n_retf_swap",
    "n_retf_failafter",
    "n_retf_relink2",
];

/// BUNDLE-N: every `n_retf_*` program in the whole matrix (trace modes, exit storms, random allocation, budgets, GC stress 1 / 2 in both collector
/// modes, stack moves, the emulator) with `retf=1`, `retf=0` (the control) and the process default (retf=3).
#[test]
fn nr_matrix() {
    let only = std::env::var("TR_CORE_ONLY").ok();
    for p in NR_PROGNAMES
        .iter()
        .filter(|p| only.as_ref().is_none_or(|o| p.contains(o.as_str())))
    {
        assert!(!compare(ba_jobs(
            p,
            &[
                "retf=1,retfmax=1",
                "retf=1,retfmax=2",
                "retf=3,retfmax=2",
                "retf=7,retfmax=3",
                "retf=0",
                ""
            ]
        ))
        .is_empty());
    }
}

/// BUNDLE-N: a callee loop that exits often gets a side trace that pops into the caller (`retf` counts the installed traces with a `RETF`), and the
/// run's output equals the interpreter's; with the knob off there is none. Mutants `retf_slot_remap_off`, `retf_results_dest_off`,
/// `retf_known_unshifted`, `retf_model_slots_unshifted`, `retf_multret_top_unmapped`, `retf_root_func_stale` (the lockstep aborts the recording
/// or the output changes).
#[test]
fn nr_retf_traces_are_installed() {
    let mk = |l: &str| Job::new("n_retf_hot", "always").opts("+verify").limits_s(l);
    let r = run_all(vec![mk("retf=1,retfmax=1"), mk("retf=0")]);
    assert!(r[0].1.ok && r[1].1.ok, "{}", r[0].1.stderr);
    // RETF is inert under the 5.4 build (by design): no RETF trace is ever installed, the output is the same
    let inert = cfg!(rustynail_lua54);
    if inert {
        assert_eq!(
            r[0].1.stats.retf,
            0,
            "RETF is inert on 5.4: {}",
            r[0].1.stats.line()
        );
    } else {
        assert!(
            r[0].1.stats.retf >= 1,
            "no RETF trace installed: {}",
            r[0].1.stats.line()
        );
    }
    assert_eq!(r[1].1.stats.retf, 0);
    assert_eq!(r[0].1.result, r[1].1.result);
    for p in [
        "n_retf_basic",
        "n_retf_tail",
        "n_retf_two_sites",
        "n_retf_mr_hot",
        "n_retf_continue",
        "n_retf_two_levels",
    ] {
        let r = run_all(vec![Job::new(p, "always")
            .opts("+verify")
            .limits_s("retf=1,retfmax=1")]);
        assert!(r[0].1.ok, "{p}: {}", r[0].1.stderr);
        if inert {
            assert_eq!(
                r[0].1.stats.retf,
                0,
                "{p}: RETF is inert on 5.4: {}",
                r[0].1.stats.line()
            );
        } else {
            assert!(
                r[0].1.stats.retf >= 1,
                "{p}: no RETF trace installed: {}",
                r[0].1.stats.line()
            );
        }
    }
}

/// BUNDLE-N-B (N3a/N3b): the programs that need the recording to go on in the caller (types of the caller's registers, the caller's closure, two levels, caller loop
/// heads, the re-pushed callee) under `retf=7`, with exit storms and `+verify` (the segment oracle), against the knob off. RETF traces must be installed.
/// Mutants of the continued recording: `retf_known_unshifted`, `retf_func_ref_stale`, `retf_model_func_stale`, `retf_entry_hoist`, `retf_entry_checks_past`,
/// `retf_untraced_aborts`, `retf_second_return_aborts`, `relink_jumps_to_land`, `relink_savedpc_off`.
#[test]
fn nr_n3_programs() {
    let lim = "retf=7,retfmax=3";
    for p in [
        "n_retf_types",
        "n_retf_upvals",
        "n_retf_continue",
        "n_retf_two_levels",
        "n_retf_relink",
        "n_retf_untraced",
        "n_retf_upvalue_alias",
        "n_retf_recursion",
        "n_retf_poly",
        "n_retf_swap",
        "n_retf_failafter",
        "n_retf_relink2",
    ] {
        let jobs = vec![
            Job::new(p, "always").opts("+verify").limits_s(lim),
            Job::new(p, "always")
                .opts("+exitstorm=3 +verify")
                .limits_s(lim),
            Job::new(p, "always")
                .opts("+exitstorm=7 +randra=5 +verify")
                .limits_s(lim),
            Job::new(p, "always").opts("+verify").limits_s("retf=0"),
        ];
        let r = run_all(jobs);
        let off = r.last().unwrap().1.clone();
        assert!(off.ok, "{p}: {}", off.stderr);
        for (j, o) in &r[..3] {
            assert!(o.ok, "{p} [{}]: {}", j.opts, o.stderr);
            assert_eq!(
                o.result, off.result,
                "{p} [{}]: output differs from the knob-off run",
                j.opts
            );
        }
        if cfg!(rustynail_lua54) {
            // RETF is inert on 5.4 (by design): none installed, output equal to the knob-off run (checked above)
            assert_eq!(
                r[0].1.stats.retf,
                0,
                "{p}: RETF is inert on 5.4: {}",
                r[0].1.stats.line()
            );
        } else if !matches!(
            p,
            "n_retf_upvalue_alias" | "n_retf_recursion" | "n_retf_relink2" | "n_retf_failafter"
        ) {
            assert!(
                r[0].1.stats.retf >= 1,
                "{p}: no RETF trace installed: {}",
                r[0].1.stats.line()
            );
        }
    }
}

/// N3b (`retf` bit 1): the frame-linking exit of a side trace (the "re-push" into a callee loop trace) is an inline stub: it rebuilds the frames itself and jumps through
/// the head's dispatch cell. Programs `c38_side`, `c38_side2` with `sidenest=1,retf=2` (no RETF recorded): same output as the interpreter in every mode, and
/// the callee's loop trace is entered as often as with the handler path. Mutants `relink_jumps_to_land` (the stub lands in tier 1 behind the head: the loop trace
/// is entered less), `relink_savedpc_off`.
#[test]
fn n3b_relink_stub_programs() {
    let mut jobs = Vec::new();
    for p in ["c38_side", "c38_side2"] {
        for trace in ["always", "on2"] {
            for lim in ["sidenest=1,retf=0", "sidenest=1,retf=2"] {
                jobs.push(Job::new(p, trace).opts("+verify").limits_s(lim));
                jobs.push(
                    Job::new(p, trace)
                        .opts("+exitstorm=3 +randra=5")
                        .limits_s(lim),
                );
                jobs.push(Job::new(p, trace).budget(17).limits_s(lim));
                jobs.push(
                    Job::new(p, trace)
                        .env("RUSTYNAIL_GC_STRESS", "2")
                        .limits_s(lim),
                );
                jobs.push(Job::new(p, trace).opts("+emurun +verify").limits_s(lim));
            }
        }
    }
    let res = compare(jobs);
    let find = |p: &str, trace: &str, lim: &str| {
        res.iter()
            .find(|(j, _)| {
                j.prog == p
                    && j.trace == trace
                    && j.limits == lim
                    && j.opts == "+verify"
                    && j.env.is_empty()
            })
            .map(|(_, o)| o.stats.clone())
            .expect("job")
    };
    for p in ["c38_side", "c38_side2"] {
        for trace in ["always", "on2"] {
            let (h, st) = (
                find(p, trace, "sidenest=1,retf=0"),
                find(p, trace, "sidenest=1,retf=2"),
            );
            eprintln!("{p} [{trace}] handler: {} | stub: {}", h.line(), st.line());
            if p == "c38_side" {
                assert!(
                    st.side_links >= 1,
                    "{p} [{trace}]: no linking side trace: {}",
                    st.line()
                );
                assert!(
                    20 * st.entered >= 19 * h.entered,
                    "{p} [{trace}]: the loop trace is entered less through the stub: {} vs {}",
                    st.entered,
                    h.entered
                );
            }
        }
    }
}
