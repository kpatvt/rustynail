//! Shared infrastructure of the P8a test binaries (`tr_oracle.rs`, `tr_identity.rs`, `tr_mutants.rs`): the corpus,
//! the per-job child processes (the test binary re-executed with an environment variable, so `os.exit`, crashes,
//! timeouts and the process-wide hooks are contained), state construction for every trace mode and the interpreter
//! fetch counter of `tr_identity_pre.rs` (the `t15_oracle` hook plus the two unfetched `TFOR*` chains).
//!
//! Included with `#[path = "tr_p8a/common.rs"] mod common;`; items a binary does not use are allowed to be dead.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn repo_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Small `arg[1]` per benchmark (no big allocations; same values as `tr_identity_pre.rs`).
pub fn bench_arg(name: &str) -> &'static str {
    match name {
        "binary_trees" => "6",
        "closures" | "metamethods" | "oop_methods" | "varargs" => "20000",
        "coroutines" | "table_array" | "table_hash" => "3000",
        "fannkuch" | "queens" => "6",
        "fasta" | "knucleotide" | "string_ops" => "2000",
        "fib" => "16",
        "gc_churn" => "30",
        "heapsort" => "2000",
        "json" => "6",
        "load_big" => "20",
        "loops_arith" => "20000",
        "mandelbrot" => "24",
        "matmul" => "12",
        "nbody" => "300",
        "pcall_error" => "5000",
        "sieve" => "20000",
        "spectral_norm" => "12",
        "licm_expr" | "array_rec" | "upval_acc" | "int_rec2" | "int_sum" | "float_poly"
        | "float_loop" | "dot" | "upval_inv" | "inline_leaf" | "inline_2" | "short_inner"
        | "type_entry" | "type_entry_stable" | "type_late" | "type_late_stable" => "30",
        _ => "10",
    }
}

/// Programs whose instruction count is not deterministic even without a JIT (`pairs` order over string keys depends
/// on the per-state hash seed): excluded.
pub const NONDET: [&str; 1] = ["bench/programs/knucleotide"];

/// One corpus program. `spec` is `kind|path-or-name|arg`; `kind` is `prog` (a Lua file), `bench` (a Lua file with
/// `arg = {"<arg>"}`), `gen` (the jit-diff generator run with seed `arg`) or `core` (a program of `tr_core/progs.rs`,
/// by name, run under the capturing prelude).
#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub spec: String,
}

/// Lists the corpus. `TR_P8A_ONLY=<substr>` selects programs; `TR_P8A_GEN=<n>` sets the number of generator seeds.
pub fn corpus(core_names: &[&str], gen_default: u32) -> Vec<Item> {
    let root = repo_root();
    let only = std::env::var("TR_P8A_ONLY").ok();
    let mut jobs = Vec::new();
    for (dir, kind) in [
        ("tests/tr_oracle_progs", "prog"),
        ("tests/t15_oracle_progs", "prog"),
        ("tests/vm", "prog"),
        ("bench/programs", "bench"),
        ("bench/jit-trace", "bench"),
    ] {
        let mut files: Vec<_> = std::fs::read_dir(root.join(dir))
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for f in files {
            if f.extension().and_then(|e| e.to_str()) != Some("lua") {
                continue;
            }
            let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
            let arg = if kind == "bench" {
                bench_arg(&stem)
            } else {
                ""
            };
            jobs.push(Item {
                name: format!("{dir}/{stem}"),
                spec: format!("{kind}|{}|{arg}", f.display()),
            });
        }
    }
    for n in core_names {
        jobs.push(Item {
            name: format!("core/{n}"),
            spec: format!("core|{n}|"),
        });
    }
    let n_gen: u32 = std::env::var("TR_P8A_GEN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(gen_default);
    let gen = root.join("tests/jitdiff/gen.lua");
    for seed in 1..=n_gen {
        jobs.push(Item {
            name: format!("gen:{seed}"),
            spec: format!("gen|{}|{seed}", gen.display()),
        });
    }
    jobs.retain(|j| !NONDET.contains(&j.name.as_str()));
    if let Some(o) = only {
        jobs.retain(|j| j.name.contains(&o));
    }
    jobs
}

// ---- running a program in a state ----------------------------------------------------------------------------------

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::tr::cache::{TrConfig, TrMode};
use rn_jit::tr::TraceOpts;
use rn_jit::{Config, Mode};

/// Lua prelude of the `core` programs: captured `print` / `io.write` (as `tr_core.rs`).
pub const CORE_PRELUDE: &str = "local __b = {} \
 print = function(...) local n = select('#', ...) for i = 1, n do __b[#__b+1] = tostring((select(i, ...))) if i < n then __b[#__b+1] = '\\t' end end __b[#__b+1] = '\\n' end \
 io.write = function(...) for i = 1, select('#', ...) do __b[#__b+1] = tostring((select(i, ...))) end end \
 function __result() return table.concat(__b) end ";

/// A loaded program: the chunk to run, an optional prelude chunk, the chunk name.
pub struct Loaded {
    pub src: Vec<u8>,
    pub prelude: String,
    pub path: String,
}

pub fn load(spec: &str, core_src: &dyn Fn(&str) -> &'static str) -> Result<Loaded, String> {
    let mut it = spec.splitn(3, '|');
    let kind = it.next().unwrap_or("");
    let path = it.next().unwrap_or("");
    let arg = it.next().unwrap_or("");
    let _ = std::env::set_current_dir(repo_root());
    match kind {
        "gen" => {
            let mut st = rn_std::new_state_with_libs().expect("state");
            rn_jit::set_mode(&mut st, Mode::Off);
            let wrapper = format!(
                "arg = {{\"{arg}\"}}\nlocal buf = {{}}\nio.write = function(...)\n for i = 1, select('#', ...) do buf[#buf + 1] = tostring((select(i, ...))) end\nend\ndofile(\"{path}\")\nreturn table.concat(buf)\n"
            );
            match run_chunk(&mut st, wrapper.as_bytes(), b"=gen") {
                (Status::Ok, Some(o)) => Ok(Loaded {
                    src: o.into_bytes(),
                    prelude: String::new(),
                    path: path.into(),
                }),
                r => Err(format!("generator failed: {r:?}")),
            }
        }
        "core" => {
            let src = core_src(path);
            Ok(Loaded {
                src: format!(
                    "{CORE_PRELUDE}local __ok, __e = xpcall(function(...)\n{src}\nend, debug.traceback)\n\
                     return __result() .. 'STATUS ' .. tostring(__ok) .. ' ' .. tostring(__e) .. '\\n'"
                )
                .into_bytes(),
                prelude: String::new(),
                path: format!("core/{path}"),
            })
        }
        k => {
            let src = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            Ok(Loaded {
                src,
                prelude: if k == "bench" {
                    format!("arg = {{\"{arg}\"}}\n")
                } else {
                    String::new()
                },
                path: path.into(),
            })
        }
    }
}

pub fn run_chunk(st: &mut LuaState, src: &[u8], name: &[u8]) -> (Status, Option<String>) {
    let base = st.top();
    let s = st.load_buffer(src, name, None);
    if s != Status::Ok {
        st.set_top(base);
        return (s, None);
    }
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = v
        .as_string()
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned());
    st.set_top(base);
    (status, out)
}

/// One configuration of a run. `mode`: `off` (JIT off), `t1` (JIT always, traces off), `always` (JIT always, traces
/// always), `on2` (JIT always, traces on with hot thresholds 2,2), `on` (JIT on with default thresholds, traces on).
#[derive(Clone, Debug, Default)]
pub struct Cfg {
    pub mode: String,
    pub opts: String,
    pub budget: Option<i32>,
}

impl Cfg {
    pub fn new(mode: &str, opts: &str, budget: Option<i32>) -> Cfg {
        Cfg {
            mode: mode.into(),
            opts: opts.into(),
            budget,
        }
    }
    pub fn traced(&self) -> bool {
        matches!(self.mode.as_str(), "always" | "on2" | "on")
    }
    pub fn tag(&self) -> String {
        format!(
            "{} {} budget={}",
            self.mode,
            self.opts,
            self.budget.map_or("none".into(), |b| b.to_string())
        )
    }
}

pub fn trace_cfg(c: &Cfg) -> TrConfig {
    let mode = if c.mode == "always" {
        TrMode::Always
    } else {
        TrMode::On
    };
    let mut cfg = TrConfig::for_mode(mode);
    if c.mode == "on2" {
        cfg.hot_loop = 2;
        cfg.hot_exit = 2;
    }
    cfg.opts = TraceOpts::parse(&c.opts).expect("options");
    // (P-E2: `TR_P8A_LIMITS=fnroot=1,...` turns the function-entry roots on, they are off by default)
    if let Ok(l) = std::env::var("TR_P8A_LIMITS") {
        cfg.limits = rn_jit::tr::cache::TrLimits::parse(&l).0;
    }
    cfg.verbose = 1;
    cfg
}

/// A state in configuration `c` (tier-1.5 regions off, as `tr_core.rs`).
pub fn make_state(c: &Cfg) -> LuaState {
    // a fixed string-hash seed: `pairs` order over string keys must not differ between the compared runs
    let mut st = rn_std::new_state(0x5eed_1234).expect("state");
    rn_std::open_libs(&mut st).expect("libs");
    rn_jit::set_t15(
        &mut st,
        rn_jit::t15::runtime::T15Config::for_mode(rn_jit::t15::runtime::T15Mode::Off),
    );
    if c.mode == "off" {
        rn_jit::set_mode(&mut st, Mode::Off);
        return st;
    }
    rn_jit::set_mode(
        &mut st,
        if c.mode == "on" {
            Mode::On
        } else {
            Mode::Always
        },
    );
    if c.traced() {
        rn_jit::rt::set_trace(&mut st, trace_cfg(c));
        rn_jit::tr::attach_driver(&mut st);
    } else {
        rn_jit::rt::set_trace(&mut st, TrConfig::for_mode(TrMode::Off));
    }
    if let Some(b) = c.budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    st
}

/// Process setup of every child: memory cap (CLAUDE.md), JIT registration, the trace driver, the rn-core mutants.
pub fn child_setup(on_mode: bool) {
    // address-space cap (`TR_P8A_AS_MB`, default 2 GB: the parent runs several children at once)
    let mb: u64 = std::env::var("TR_P8A_AS_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2048);
    // SAFETY: plain libc call.
    unsafe {
        let lim = libc::rlimit {
            rlim_cur: mb << 20,
            rlim_max: mb << 20,
        };
        libc::setrlimit(libc::RLIMIT_AS, &lim);
    }
    let cfg = Config::for_mode(if on_mode { Mode::On } else { Mode::Always });
    assert!(rn_jit::register_with_trace_pinned_off(cfg), "register");
    rn_jit::tr::install_driver();
    #[cfg(feature = "tr_mutants")]
    rn_jit::tr::mutants::apply_vm_mutants();
}

// ---- the interpreter fetch counter ---------------------------------------------------------------------------------

#[cfg(feature = "t15_oracle")]
pub mod count {
    //! `fetches` of docs/JIT_TRACE.md 9.5: the instructions the interpreter fetches (the `t15_oracle` hook) plus the
    //! `TFORCALL` / `TFORLOOP` chains it executes without fetching (`rn_core::oracle::unfetched`, counted by the
    //! interpreter when it executes them, Q5).
    use std::sync::atomic::{AtomicU64, Ordering};
    pub static FETCHES: AtomicU64 = AtomicU64::new(0);

    pub fn reset() {
        FETCHES.store(0, Ordering::SeqCst);
        rn_core::oracle::reset_unfetched();
    }

    /// Instructions executed so far by the interpreter (fetched or chained).
    pub fn total() -> u64 {
        FETCHES.load(Ordering::SeqCst) + rn_core::oracle::unfetched()
    }

    /// Accounts one fetch and returns the number of instructions the interpreter had executed BEFORE it (the
    /// position of this instruction in the interpreter-only timeline when no compiled code ran).
    pub fn on_fetch(_frame: usize, _p: *const rn_core::func::Proto, _pc: usize) -> u64 {
        let old = FETCHES.fetch_add(1, Ordering::Relaxed);
        old + rn_core::oracle::unfetched()
    }

    /// The plain counting hook (identity runs).
    pub fn hook(
        frame: usize,
        p: *const rn_core::func::Proto,
        pc: usize,
        _base: *mut rn_core::value::Value,
        _ms: usize,
    ) {
        on_fetch(frame, p, pc);
    }
}

// ---- children -------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ChildOut {
    /// The child exited with status 0.
    pub ok: bool,
    pub timed_out: bool,
    /// Killed by a signal (crash / abort).
    pub signalled: bool,
    pub stderr: String,
}

/// Runs the test binary `exe` with `--exact <test>` and the environment `env` (stdin and stdout are null; the
/// parent reads stderr, where the child writes its report lines); `timeout` seconds, then the child is killed.
pub fn run_child(
    exe: &std::path::Path,
    test: &str,
    env: &[(String, String)],
    timeout: u64,
) -> ChildOut {
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env_remove("RUSTYNAIL_JIT")
        .env_remove("RUSTYNAIL_JIT_TRACE")
        .env_remove("RUSTYNAIL_GC_STRESS")
        .env_remove("RUSTYNAIL_JIT_STRESS")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn child");
    let mut se = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        // keep only the marker lines once a chatty program produced a lot of output
        let mut buf = Vec::new();
        let mut chunk = [0u8; 65536];
        loop {
            match se.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > 4 << 20 {
                        buf = buf
                            .split(|b| *b == b'\n')
                            .filter(|l| l.starts_with(b"P8A") || l.starts_with(b"[trace]"))
                            .flat_map(|l| l.iter().copied().chain(std::iter::once(b'\n')))
                            .collect();
                    }
                }
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let t0 = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait().expect("wait") {
            Some(s) => break s,
            None if t0.elapsed() > Duration::from_secs(timeout) => {
                let _ = child.kill();
                timed_out = true;
                break child.wait().expect("wait");
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    use std::os::unix::process::ExitStatusExt;
    ChildOut {
        ok: status.success(),
        timed_out,
        signalled: status.signal().is_some() && !timed_out,
        stderr: reader.join().unwrap(),
    }
}

pub struct Job {
    pub label: String,
    pub env: Vec<(String, String)>,
}

/// Runs the jobs on a few worker threads (default at most 3, `TR_P8A_JOBS` overrides; several tests run at once, each with
/// its own pool, so a larger default saturates the machine); results in job order.
pub fn run_pool(
    exe: &std::path::Path,
    test: &str,
    jobs: Vec<Job>,
    timeout: u64,
) -> Vec<(String, ChildOut)> {
    let n = std::env::var("TR_P8A_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(2, |n| n.get())
                .clamp(1, 3)
        });
    let queue = std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>());
    let results = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| loop {
                let next = queue.lock().unwrap().pop();
                let Some((i, j)) = next else { break };
                let o = run_child(exe, test, &j.env, timeout);
                results.lock().unwrap().push((i, j.label, o));
            });
        }
    });
    let mut v = results.into_inner().unwrap();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|(_, l, o)| (l, o)).collect()
}

pub fn timeout_secs(default: u64) -> u64 {
    std::env::var("TR_P8A_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The lines of `err` that start with `prefix` (marker lines of a child).
pub fn marker_lines<'a>(err: &'a str, prefix: &'a str) -> impl Iterator<Item = &'a str> {
    err.lines().filter(move |l| l.starts_with(prefix))
}

/// What the child's `[trace]` summaries said, summed over the states it ran.
#[derive(Clone, Debug, Default)]
pub struct TraceSum {
    pub recorded: u64,
    pub installed: u64,
    pub entered: u64,
    pub exits: u64,
}

impl TraceSum {
    pub fn parse(err: &str) -> TraceSum {
        let mut s = TraceSum::default();
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
                s.recorded += w.first().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                s.installed += g("installed");
                s.entered += g("entered");
            } else if let Some(r) = l.strip_prefix("exits ") {
                s.exits += r
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
        s
    }
}

pub fn tail(s: &str, n: usize) -> String {
    let v: Vec<&str> = s.lines().collect();
    v[v.len().saturating_sub(n)..].join("\n")
}

/// Groups results by a key (used for per-suite summaries).
pub fn group<K: Ord, V>(it: impl IntoIterator<Item = (K, V)>) -> BTreeMap<K, Vec<V>> {
    let mut m: BTreeMap<K, Vec<V>> = BTreeMap::new();
    for (k, v) in it {
        m.entry(k).or_default().push(v);
    }
    m
}

/// The peak resident set of this process in kB (`VmHWM`), 0 if unknown.
pub fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("VmHWM:"))
                .and_then(|v| v.split_whitespace().next().and_then(|x| x.parse().ok()))
        })
        .unwrap_or(0)
}
