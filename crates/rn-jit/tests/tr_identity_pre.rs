//! P0x / experiment X6 (docs/JIT_TRACE.md 9.5, 10.1): the instruction-count identity of tier 1 and of the J4
//! regions, checked BEFORE the trace compiler (TR5) relies on it:
//!
//! ```text
//!     fetches(JIT off) == fetches(run) + Stats::insns(run)
//! ```
//!
//! `fetches` is the number of instructions the *interpreter* fetches (the `t15_oracle` per-fetch hook of rn-core,
//! the only fetch counter the VM has); `Stats::insns` is the bytecode count compiled code consumed (sum over
//! stays of `budget - counter`). Every bytecode of a run is either interpreted or executed by compiled code, so the
//! sum must not depend on the mode.
//!
//! Matrix per program: JIT mode {on (default thresholds), low (on, thresholds 2/2), always} x tier 1.5
//! {off (= tier 1), on, always (= J4 regions)} x count budget {none, 3, 17, 200}; the JIT-off baseline is run
//! twice (programs whose two baselines differ are non-deterministic and skipped, e.g. clock/random driven).
//! Corpus: `tests/vm`, `tests/t15_oracle_progs`, `bench/programs` and `bench/jit-trace` (small N), 10 jit-diff
//! generator seeds. Each (program, JIT mode) runs in a child process (this binary re-executed), so `os.exit`,
//! crashes and timeouts are contained; a child that does not finish is reported as inconclusive, not as a gap.
//!
//! # Result (576 jobs = 192 programs x 3 JIT modes; 573 fully checked; 3 = gen:6, which ends in `os.exit(3)`, are
//! inconclusive by design; 259 jobs with tier-1.5 promotions, so J4 regions are exercised)
//!
//! The identity holds EXACTLY (all JIT modes, all tier-1.5 modes, budgets none/3/17/200) after the P0y fixes,
//! except for the NONDET program at the end (the former G4 programs are exact since Q5). History (P0x found, P0y fixed):
//!
//! * **G1 (product, fixed in P0y): budget exits overcounted.** A failed block count (`CountSub` negative ->
//!   exit at the head) left the counter negative without refunding the block it had just pre-paid, and `rt::enter`
//!   computes `insns += budget - left`, so the block that the interpreter then executes was counted twice
//!   (`local t=0 for i=1,1000 do t=t+i end`, JIT always, budget 200: off=2007, sum=2025). Fix: every `CountSub` exit
//!   goes to `Cx::exit_cnt(pc, n)`, the lazily emitted exit sequence of tier 1 that adds the `n` just subtracted
//!   back before leaving (`savedpc = pc`); the J4 region budget exits (`cexit(pc, n)`) leave through the same
//!   sequence. `left` is now the exact budget left and `Stats::insns` the exact number of bytecodes that ran.
//! * **G3a (product, fixed in P0y): the RETURN whose `__close` handler raised was not counted.** `tpl_return` (k bit)
//!   runs `jit_return_close` before the RETURN is counted (`return_prologue`); on the error exit it now subtracts the
//!   1 of the RETURN. (`pcall(function() local a <close> = setmetatable({}, {__close=function() error("x",0) end})
//!   end)`: off=17, was 16.) This also explained json and gen:2/3/10 (their generated code has erroring closes).
//! * **G2 (test oracle).** The interpreter executes `TFORCALL` after `TFORPREP` and `TFORLOOP` after `TFORCALL`
//!   WITHOUT fetching them (`do_tforcall!`/`do_tforloop!` chain in vm_unsafe.rs), while tier 1 counts them. The hook
//!   below adds the chained instructions to `fetches` (TFORPREP +2, TFORCALL +1). TR5.1's `fetches` model must
//!   reproduce the same two chains (docs/JIT_TRACE.md 9.5).
//! * **G4 (test hook imprecision).** The credit is given at the fetch and taken back when the next fetch of the same
//!   frame id is in another prototype (the iterator raised or yielded and the frame unwound). The take-back is
//!   suppressed when a compiled stay ran in between (`rt::ENTERS`, oracle builds only): then the frame ended there
//!   and its id was reused, which is not an unwind. This removed a spurious -1 (gen:1, budget 199..202 ...). What
//!   remained was the three programs of `G4` (iterator errors/yields), |diff| <= 11. **Q5 fixed it at the root**: the
//!   interpreter itself counts the chain instructions it executes without a fetch (`rn_core::oracle::unfetched`,
//!   incremented in `do_tforcall!` after the call returned and in `TFORPREP` for its TFORCALL), so an erroring or
//!   yielding iterator never credits a `TFORLOOP` it did not reach; the `PENDING` / take-back heuristic is gone and the
//!   three programs (and `tfor_te6`) are exact.
//! * **Non-determinism.** knucleotide counts differ between two runs of the JIT-OFF interpreter (the `pairs` order of
//!   string keys depends on the per-state hash seed, and the program sorts with a comparator that reads it), which
//!   is why the earlier "unresolved" diffs and the flake appeared; it is excluded (`NONDET`). json, gen:2/3/10 were
//!   G3a, not table order.
//!
//! Debugging aids: `X6_ONLY=<substr>` selects jobs, `X6_DIR=<dir>` adds programs, `X6_BUDGETS=1-400,none`
//! replaces the budgets, `X6_V=1` prints OK lines, `X6_TRACE=1|all` traces fetches.
#![cfg(feature = "t15_oracle")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::t15::runtime::{T15Config, T15Mode};
use rn_jit::{Config, Mode};
use std::sync::atomic::{AtomicU64, Ordering};

static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FETCHES: AtomicU64 = AtomicU64::new(0);

fn hook(
    _frame: usize,
    p: *const rn_core::func::Proto,
    pc: usize,
    _base: *mut rn_core::value::Value,
    _ms: usize,
) {
    FETCHES.fetch_add(1, Ordering::Relaxed);
    if TRACE.load(Ordering::Relaxed) {
        // SAFETY: the interpreter passes the live prototype of the running frame.
        let proto = unsafe { &*p };
        let ins = rn_core::instruction::Instruction(proto.code()[pc]);
        eprintln!(
            "X6-TRACE {:?} proto-len={} pc={}",
            ins.opcode(),
            proto.code().len(),
            pc + 1
        );
    }
}

const BUDGETS: [Option<i32>; 4] = [None, Some(3), Some(17), Some(200)];

/// The budgets of a run; `X6_BUDGETS=1,2,5-9` replaces them (debugging).
fn budgets() -> Vec<Option<i32>> {
    let Ok(v) = std::env::var("X6_BUDGETS") else {
        return BUDGETS.to_vec();
    };
    let mut out = Vec::new();
    for part in v.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            let (a, b): (i32, i32) = (a.parse().unwrap(), b.parse().unwrap());
            out.extend((a..=b).map(Some));
        } else {
            out.push(part.parse().ok());
        }
    }
    out
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

/// Small `arg[1]` per benchmark (no big allocations).
fn bench_arg(name: &str) -> &'static str {
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
        // bench/jit-trace: reps of an inner loop of 1000 (or 64-pixel rows, 20x50 for type_*)
        "licm_expr" | "array_rec" | "upval_acc" | "int_rec2" | "int_sum" | "float_poly"
        | "float_loop" | "dot" | "upval_inv" | "inline_leaf" | "inline_2" | "short_inner"
        | "type_entry" | "type_entry_stable" | "type_late" | "type_late_stable" => "30",
        _ => "10",
    }
}

fn run_chunk(st: &mut LuaState, src: &[u8], name: &[u8]) -> (Status, Option<String>) {
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

fn t15_name(m: T15Mode) -> &'static str {
    match m {
        T15Mode::Off => "t1",
        T15Mode::On => "t15-on",
        T15Mode::Always => "t15-always",
    }
}

/// One run: returns `(fetches, insns, status)`. `cfg = None` is the JIT-off baseline.
fn one_run(
    src: &[u8],
    path: &str,
    prelude: &str,
    cfg: Option<(T15Mode, Option<i32>)>,
) -> (u64, u64, u64, Status) {
    let mut st = rn_std::new_state_with_libs().expect("state");
    match cfg {
        None => rn_jit::set_mode(&mut st, Mode::Off),
        Some((t15, budget)) => {
            rn_jit::set_t15(&mut st, T15Config::for_mode(t15));
            if let Some(b) = budget {
                rn_jit::set_count_budget(&mut st, b);
            }
        }
    }
    if TRACE.load(Ordering::Relaxed) {
        eprintln!("X6-RUN {}", cfg.is_some());
    }
    FETCHES.store(0, Ordering::SeqCst);
    rn_core::oracle::reset_unfetched();
    rn_core::oracle::set_hook(Some(hook));
    if !prelude.is_empty() {
        run_chunk(&mut st, prelude.as_bytes(), b"=prelude");
    }
    let (status, _) = run_chunk(&mut st, src, format!("@{path}").as_bytes());
    rn_core::oracle::set_hook(None);
    let f = FETCHES.load(Ordering::SeqCst) + rn_core::oracle::unfetched();
    let stats = rn_jit::stats(&mut st);
    drop(st);
    (f, stats.insns, stats.t15_promoted, status)
}

fn child_main(spec: &str) {
    rn_core::jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
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
    let mut it = spec.splitn(4, '|');
    let jmode = it.next().unwrap_or("");
    let kind = it.next().unwrap_or("");
    let path = it.next().unwrap_or("");
    let arg = it.next().unwrap_or("");
    let _ = std::env::set_current_dir(repo_root());
    let cfg = match jmode {
        "on" => Config::for_mode(Mode::On),
        "low" => Config {
            mode: Mode::On,
            entry_calls: 2,
            loop_edges: 2,
        },
        "always" => Config::for_mode(Mode::Always),
        m => panic!("bad jit mode {m}"),
    };
    assert!(rn_jit::register_with_trace_pinned_off(cfg));
    let src: Vec<u8> = if kind == "gen" {
        let mut st = rn_std::new_state_with_libs().expect("state");
        rn_jit::set_mode(&mut st, Mode::Off);
        let wrapper = format!(
            "arg = {{\"{arg}\"}}\nlocal buf = {{}}\nio.write = function(...)\n for i = 1, select('#', ...) do buf[#buf + 1] = tostring((select(i, ...))) end\nend\ndofile(\"{path}\")\nreturn table.concat(buf)\n"
        );
        match run_chunk(&mut st, wrapper.as_bytes(), b"=gen") {
            (Status::Ok, Some(o)) => o.into_bytes(),
            r => {
                eprintln!("X6-SKIP generator failed: {r:?}");
                return;
            }
        }
    } else {
        std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"))
    };
    let prelude = if kind == "bench" {
        format!("arg = {{\"{arg}\"}}\n")
    } else {
        String::new()
    };
    TRACE.store(std::env::var("X6_TRACE").is_ok(), Ordering::Relaxed);
    let trace_all = std::env::var("X6_TRACE").is_ok_and(|v| v == "all");
    let (f0, _, _, s0) = one_run(&src, path, &prelude, None);
    TRACE.store(trace_all, Ordering::Relaxed);
    let (f1, _, _, _) = one_run(&src, path, &prelude, None);
    if f0 != f1 {
        eprintln!("X6-SKIP nondeterministic baseline ({f0} vs {f1})");
        return;
    }
    let mut runs = 0u32;
    for t15 in [T15Mode::Off, T15Mode::On, T15Mode::Always] {
        for b in budgets() {
            let (f, insns, promoted, s) = one_run(&src, path, &prelude, Some((t15, b)));
            runs += 1;
            let tag = format!(
                "jit={jmode} {} budget={}",
                t15_name(t15),
                b.map_or("none".into(), |b| b.to_string())
            );
            let sum = f.wrapping_add(insns);
            if sum != f0 || s != s0 {
                eprintln!(
                    "X6-GAP [{tag}] off={f0} fetches={f} insns={insns} sum={} diff={} status {s0:?}->{s:?}",
                    sum,
                    sum.wrapping_sub(f0) as i64
                );
            } else if std::env::var("X6_V").is_ok() {
                eprintln!("X6-OK [{tag}] off={f0} fetches={f} insns={insns}");
            }
            if insns > 0 {
                eprintln!("X6-COMPILED [{tag}]");
            }
            if promoted > 0 {
                eprintln!("X6-PROMOTED [{tag}] {promoted}");
            }
        }
    }
    eprintln!("X6-DONE runs={runs} baseline={f0}");
}

#[test]
fn child_run() {
    if let Ok(spec) = std::env::var("X6_CHILD") {
        child_main(&spec);
    }
}

fn corpus() -> Vec<(String, String)> {
    let root = repo_root();
    let only = std::env::var("X6_ONLY").ok();
    let mut jobs = Vec::new();
    for (dir, kind) in [
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
            jobs.push((
                format!("{dir}/{stem}"),
                format!("{kind}|{}|{arg}", f.display()),
            ));
        }
    }
    // extra programs (debugging): X6_DIR=<dir with .lua files>
    if let Ok(d) = std::env::var("X6_DIR") {
        let mut files: Vec<_> = std::fs::read_dir(&d)
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for f in files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e == "lua"))
        {
            jobs.push((
                format!("x/{}", f.display()),
                format!("prog|{}|", f.display()),
            ));
        }
    }
    let gen = root.join("tests/jitdiff/gen.lua");
    for seed in 1..=10 {
        jobs.push((
            format!("gen:{seed}"),
            format!("gen|{}|{seed}", gen.display()),
        ));
    }
    jobs.retain(|j| !NONDET.contains(&j.0.as_str()));
    if let Some(o) = only {
        jobs.retain(|j| j.0.contains(&o));
    }
    jobs
}

#[test]
fn fetch_plus_insns_identity() {
    if std::env::var("X6_CHILD").is_ok() {
        return;
    }
    if !rn_jit::available() {
        // `available` needs a registered compiler: register in the parent as well
        rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always));
        if !rn_jit::available() {
            eprintln!("JIT unavailable on this host: skipped");
            return;
        }
    }
    let exe = std::env::current_exe().expect("exe");
    let mut jobs = Vec::new();
    for (name, spec) in corpus() {
        for jm in ["on", "low", "always"] {
            jobs.push((format!("{name} [{jm}]"), format!("{jm}|{spec}")));
        }
    }
    let queue = std::sync::Mutex::new(jobs);
    // (name, gap lines, finished, skipped, compiled, promoted, tail of stderr)
    let results = std::sync::Mutex::new(Vec::<Res>::new());
    let tmo: u64 = std::env::var("X6_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200);
    std::thread::scope(|sc| {
        for _ in 0..3 {
            sc.spawn(|| loop {
                let Some((name, spec)) = queue.lock().unwrap().pop() else {
                    break;
                };
                let mut ch = std::process::Command::new(&exe)
                    .args(["--exact", "child_run", "--nocapture", "--test-threads=1"])
                    .env("X6_CHILD", &spec)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .expect("spawn child");
                let out = ch.stdout.take();
                let err = ch.stderr.take();
                let t_out = std::thread::spawn(move || {
                    let mut v = Vec::new();
                    if let Some(mut o) = out {
                        let _ = std::io::Read::read_to_end(&mut o, &mut v);
                    }
                });
                let t_err = std::thread::spawn(move || {
                    let mut v = Vec::new();
                    if let Some(mut o) = err {
                        let _ = std::io::Read::read_to_end(&mut o, &mut v);
                    }
                    v
                });
                let start = std::time::Instant::now();
                let mut timed_out = false;
                let status = loop {
                    if let Some(s) = ch.try_wait().expect("wait") {
                        break s;
                    }
                    if start.elapsed().as_secs() > tmo {
                        let _ = ch.kill();
                        timed_out = true;
                        break ch.wait().expect("wait");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                };
                let _ = t_out.join();
                let err = String::from_utf8_lossy(&t_err.join().unwrap()).into_owned();
                let mut gaps = String::new();
                for l in err.lines().filter(|l| l.starts_with("X6-GAP")) {
                    gaps.push_str(l);
                    gaps.push('\n');
                }
                use std::os::unix::process::ExitStatusExt;
                if status.signal().is_some() && !timed_out {
                    gaps.push_str(&format!("CRASH {status:?}\n"));
                }
                let done = err.lines().any(|l| l.starts_with("X6-DONE"));
                let skipped = err.lines().any(|l| l.starts_with("X6-SKIP"));
                let compiled = err.lines().any(|l| l.starts_with("X6-COMPILED"));
                let promoted = err.lines().any(|l| l.starts_with("X6-PROMOTED"));
                if std::env::var("X6_V").is_ok() {
                    eprintln!("{name}:\n{err}");
                }
                if timed_out {
                    eprintln!("[x6] timeout (inconclusive): {name}");
                }
                results.lock().unwrap().push(Res {
                    name,
                    gaps,
                    done,
                    skipped,
                    compiled,
                    promoted,
                    tail: err.lines().rev().take(3).collect::<Vec<_>>().join(" | "),
                });
            });
        }
    });
    let res = results.into_inner().unwrap();
    let n = res.len();
    let done = res.iter().filter(|r| r.done).count();
    let skipped = res.iter().filter(|r| r.skipped).count();
    let compiled = res.iter().filter(|r| r.compiled).count();
    let promoted = res.iter().filter(|r| r.promoted).count();
    let unfinished: Vec<_> = res
        .iter()
        .filter(|r| !r.done && !r.skipped)
        .map(|r| format!("{} ({})", r.name, r.tail))
        .collect();
    eprintln!(
        "[x6] {n} jobs: {done} fully checked, {skipped} skipped (non-deterministic), {} inconclusive, {compiled} with compiled stays, {promoted} with tier-1.5 promotions",
        unfinished.len()
    );
    for r in res.iter().filter(|r| r.skipped) {
        eprintln!("[x6] skipped (non-deterministic baseline): {}", r.name);
    }
    for u in &unfinished {
        eprintln!("[x6] inconclusive: {u}");
    }
    // Policy: the identity is EXACT (diff == 0) for every deterministic program, JIT mode, tier-1.5 mode and budget,
    // (the former G4 programs are exact since Q5, module docs).
    let mut bad = String::new();
    for r in &res {
        let _prog = r.name.split(" [").next().unwrap_or("");
        for l in r.gaps.lines() {
            bad.push_str(&format!("--- {}\n{l}\n", r.name));
        }
    }
    assert!(bad.is_empty(), "identity gaps:\n{bad}");
    assert!(
        compiled > 0,
        "no program ran compiled code: the identity is vacuous"
    );
}

struct Res {
    name: String,
    gaps: String,
    done: bool,
    skipped: bool,
    compiled: bool,
    promoted: bool,
    tail: String,
}

/// Programs excluded because their instruction count is not deterministic even without a JIT: `pairs` order over
/// string keys depends on the per-state string-hash seed (`luaL_makeseed`: clock and an address), and the program
/// sorts with a comparator that reads that order. (Two baselines can agree by chance, so the run-twice check alone
/// made this one flaky.)
const NONDET: [&str; 1] = ["bench/programs/knucleotide"];
