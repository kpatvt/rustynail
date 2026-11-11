//! P8a: the instruction-count identity and the no-overshoot rule of the trace compiler over the whole corpus
//! (docs/JIT_TRACE.md 3.7 TR5, 9.5).
//!
//! ```text
//!     fetches(JIT off) == fetches(run) + Stats::insns(run)          (every program, trace mode, option set, budget)
//!     stay budget left >= 0                                          (no stay executes more than it was entered with)
//! ```
//!
//! `fetches` is the interpreter's fetch count (rn-core `t15_oracle` hook, plus the two unfetched `TFOR*` chains,
//! `tr_identity_pre.rs` G2); `Stats::insns` the bytecodes compiled code (tier 1 and traces) executed. The no-overshoot
//! check uses the test-only stay accounting of `rn_jit::rt::stay_stats` (feature `t15_oracle`, three counters updated
//! at the end of every `rt::enter`): a stay whose budget word ends negative executed more instructions than its budget
//! (TR5.6). Where a budget exit lands is checked per exit by `tr_oracle.rs` (landing label kind, budget word >= 0,
//! interpreter clock at the landing); here, through the identity, every landing's refund contributes to the count.
//!
//! Corpus: `tests/tr_oracle_progs`, `tests/t15_oracle_progs`, `tests/vm`, `bench/programs` and `bench/jit-trace` at small
//! N, the 42+ twin programs of `tr_core/progs.rs`, jit-diff generator seeds (`TR_P8A_GEN`, default 12). Matrix:
//!
//! * `identity_trace_modes`: trace modes `always`, `on2` (JIT always, traces on, hot 2,2) and `on` (JIT on, defaults)
//!   x budgets {none, 3, 17, 200};
//! * `identity_options`: `always` x {`-fold -cse`, `+exitstorm=2`, `+exitstorm=7`, `+randra=3`, `+membudget`,
//!   `-loop -fwd -abc -sink`} and `on2` x `+exitstorm=3 +randra=9`, x the same budgets;
//! * `identity_budget_sweep`: the twin programs with every budget 1..=40.
//!
//! Exceptions (explicit, `common.rs`): `NONDET` (knucleotide: the JIT-off baseline itself is not deterministic, the
//! per-job double baseline also skips any other such program). The identity is exact everywhere else, including the
//! programs whose iterator raises or yields inside `TFORCALL` (the former `G4`: the fetch counter now takes the unfetched
//! `TFORCALL` / `TFORLOOP` chains from the interpreter itself, `rn_core::oracle::unfetched`, Q5).
//!
//! Each job is a child process (this binary re-executed with `TR_ID_*` set): program x trace mode x option set, looping
//! over the budgets. Run: `systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1800 cargo test -p
//! rn-jit --features tr_mutants,t15_oracle --test tr_identity -- --nocapture </dev/null`. Knobs: `TR_P8A_ONLY=<substr>`,
//! `TR_P8A_GEN=<n>`, `TR_P8A_TIMEOUT=<s>`, `TR_ID_V=1` (print OK lines).
#![cfg(all(feature = "t15_oracle", feature = "trace"))]

#[path = "tr_p8a/common.rs"]
mod common;
#[allow(dead_code)]
#[path = "tr_core/progs.rs"]
mod progs;

use common::*;
use rn_core::status::Status;
use std::sync::atomic::Ordering;

const BUDGETS: [Option<i32>; 4] = [None, Some(3), Some(17), Some(200)];

fn core_src(name: &str) -> &'static str {
    progs::PROGS
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("no program {name}"))
        .src
}

fn core_names() -> Vec<&'static str> {
    progs::PROGS.iter().map(|p| p.name).collect()
}

// ---- the child ---------------------------------------------------------------------------------------------------------

struct Run {
    fetches: u64,
    insns: u64,
    status: Status,
    out: Option<String>,
    overshoots: u64,
    max_over: u64,
    /// `rt::stay_stats::INSNS` delta (must equal `insns`).
    stay_insns: u64,
    stays: u64,
}

fn one_run(l: &Loaded, cfg: &Cfg) -> Run {
    use rn_jit::rt::stay_stats as ss;
    let mut st = make_state(cfg);
    count::reset();
    let (o0, e0, i0) = (
        ss::OVERSHOOTS.load(Ordering::SeqCst),
        ss::ENDED.load(Ordering::SeqCst),
        ss::INSNS.load(Ordering::SeqCst),
    );
    ss::MAX_OVER.store(0, Ordering::SeqCst);
    rn_core::oracle::set_hook(Some(count::hook));
    if !l.prelude.is_empty() {
        run_chunk(&mut st, l.prelude.as_bytes(), b"=prelude");
    }
    let (status, out) = run_chunk(&mut st, &l.src, format!("@{}", l.path).as_bytes());
    rn_core::oracle::set_hook(None);
    let fetches = count::total();
    let insns = rn_jit::stats(&mut st).insns;
    // (read before the state closes: finalizers run by the close can still enter compiled code)
    let stay_insns = ss::INSNS.load(Ordering::SeqCst) - i0;
    drop(st); // prints the `[trace]` summary
    Run {
        fetches,
        insns,
        status,
        out,
        overshoots: ss::OVERSHOOTS.load(Ordering::SeqCst) - o0,
        max_over: ss::MAX_OVER.load(Ordering::SeqCst),
        stay_insns,
        stays: ss::ENDED.load(Ordering::SeqCst) - e0,
    }
}

fn budgets_of_env() -> Vec<Option<i32>> {
    let Ok(v) = std::env::var("TR_ID_BUDGETS") else {
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

fn child_main() {
    let spec = std::env::var("TR_ID_SPEC").unwrap();
    let mode = std::env::var("TR_ID_MODE").unwrap();
    let opts = std::env::var("TR_ID_OPTS").unwrap_or_default();
    child_setup(mode == "on");
    let l = match load(&spec, &core_src) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("P8A-SKIP {e}");
            return;
        }
    };
    let off = Cfg::new("off", "", None);
    let a = one_run(&l, &off);
    let b = one_run(&l, &off);
    if a.fetches != b.fetches {
        eprintln!(
            "P8A-SKIP nondeterministic baseline ({} vs {})",
            a.fetches, b.fetches
        );
        return;
    }
    let mut runs = 0;
    for budget in budgets_of_env() {
        let cfg = Cfg::new(&mode, &opts, budget);
        let r = one_run(&l, &cfg);
        runs += 1;
        let sum = r.fetches.wrapping_add(r.insns);
        let tag = cfg.tag();
        let diff = sum.wrapping_sub(a.fetches) as i64;
        if sum != a.fetches || r.status != a.status {
            eprintln!(
                "P8A-GAP [{tag}] off={} fetches={} insns={} sum={sum} diff={diff} status {:?}->{:?}",
                a.fetches, r.fetches, r.insns, a.status, r.status
            );
        } else if std::env::var("TR_ID_V").is_ok() {
            eprintln!(
                "P8A-OK [{tag}] off={} fetches={} insns={}",
                a.fetches, r.fetches, r.insns
            );
        }
        // for `core` programs the captured output is part of the result
        if l.path.starts_with("core/") && r.out != a.out {
            eprintln!("P8A-GAP [{tag}] output differs diff=0");
        }
        if r.overshoots > 0 {
            eprintln!(
                "P8A-OVER [{tag}] {} of {} stays ended with a negative budget (max {})",
                r.overshoots, r.stays, r.max_over
            );
        }
        if r.stay_insns != r.insns {
            eprintln!(
                "P8A-GAP [{tag}] stay_stats insns {} != Stats::insns {} diff=0",
                r.stay_insns, r.insns
            );
        }
        if r.insns > 0 {
            eprintln!("P8A-COMPILED [{tag}]");
        }
    }
    eprintln!("P8A-DONE runs={runs} baseline={}", a.fetches);
}

#[test]
fn child_entry() {
    if std::env::var("TR_ID_SPEC").is_ok() {
        child_main();
    }
}

// ---- the parent --------------------------------------------------------------------------------------------------------

struct Matrix {
    /// (mode, opts) pairs.
    cells: Vec<(&'static str, &'static str)>,
    budgets: Option<&'static str>,
    core_only: bool,
    /// Only these `tr_core` programs (and nothing else); every job must finish (the mutant probe).
    probe: Option<&'static [&'static str]>,
}

fn run_matrix(title: &str, m: &Matrix) {
    if std::env::var("TR_ID_SPEC").is_ok() {
        return;
    }
    let exe = std::env::current_exe().expect("exe");
    let names = core_names();
    let mut jobs = Vec::new();
    let items: Vec<Item> = if let Some(p) = m.probe {
        corpus(&names, 0)
            .into_iter()
            .filter(|i| p.iter().any(|n| i.name == format!("core/{n}")))
            .collect()
    } else if m.core_only {
        corpus(&names, 0)
            .into_iter()
            .filter(|i| i.name.starts_with("core/"))
            .collect()
    } else {
        corpus(&names, 12)
    };
    for it in &items {
        for (mode, opts) in &m.cells {
            let mut env = vec![
                ("TR_ID_SPEC".to_string(), it.spec.clone()),
                ("TR_ID_MODE".to_string(), (*mode).to_string()),
                ("TR_ID_OPTS".to_string(), (*opts).to_string()),
            ];
            if let Some(b) = m.budgets {
                env.push(("TR_ID_BUDGETS".into(), b.into()));
            }
            jobs.push(Job {
                label: format!("{} [{mode} {opts}]", it.name),
                env,
            });
        }
    }
    let njobs = jobs.len();
    let res = run_pool(
        &exe,
        "child_entry",
        jobs,
        timeout_secs(if m.probe.is_some() { 30 } else { 240 }),
    );
    let mut bad = String::new();
    let (mut done, mut skipped, mut inconclusive, mut compiled, mut entered_jobs, mut entered) =
        (0, 0, 0, 0, 0, 0u64);
    let mut notes = Vec::new();
    for (label, o) in &res {
        let is_done = marker_lines(&o.stderr, "P8A-DONE").next().is_some();
        let is_skip = marker_lines(&o.stderr, "P8A-SKIP").next().is_some();
        if is_done {
            done += 1;
        } else if is_skip {
            skipped += 1;
            notes.push(format!(
                "skipped: {label}: {}",
                marker_lines(&o.stderr, "P8A-SKIP").next().unwrap_or("")
            ));
        } else {
            inconclusive += 1;
            notes.push(format!(
                "inconclusive: {label} (timeout={} crash={}) {}",
                o.timed_out,
                o.signalled,
                tail(&o.stderr, 3).replace('\n', " | ")
            ));
            if o.signalled {
                bad.push_str(&format!("--- {label}\nCRASH\n{}\n", tail(&o.stderr, 12)));
            }
        }
        if marker_lines(&o.stderr, "P8A-COMPILED").next().is_some() {
            compiled += 1;
        }
        let ts = TraceSum::parse(&o.stderr);
        if ts.entered > 0 {
            entered_jobs += 1;
        }
        entered += ts.entered;
        for l in marker_lines(&o.stderr, "P8A-GAP") {
            bad.push_str(&format!("--- {label}\n{l}\n"));
        }
        for l in marker_lines(&o.stderr, "P8A-OVER") {
            bad.push_str(&format!("--- {label}\n{l}\n"));
        }
        if std::env::var("TR_ID_V").is_ok() {
            for l in marker_lines(&o.stderr, "P8A-OK") {
                eprintln!("{label}: {l}");
            }
        }
    }
    eprintln!(
        "[tr_identity] {title}: {njobs} jobs: {done} fully checked, {skipped} skipped, {inconclusive} inconclusive, \
         {compiled} with compiled stays, {entered_jobs} entered traces ({entered} trace entries)"
    );
    for n in &notes {
        eprintln!("[tr_identity]   {n}");
    }
    if m.probe.is_some() && inconclusive + skipped > 0 {
        bad.push_str(&format!(
            "probe jobs that did not finish:\n{}\n",
            notes.join("\n")
        ));
    }
    assert!(
        bad.is_empty(),
        "{title}: identity / overshoot failures:\n{bad}"
    );
    if std::env::var("TR_P8A_ONLY").is_err() && m.probe.is_none() {
        assert!(compiled > 0, "{title}: no program ran compiled code");
        assert!(
            entered_jobs * 4 >= done.max(1),
            "{title}: traces entered in only {entered_jobs} of {done} jobs: the identity is nearly vacuous"
        );
    }
}

#[test]
fn identity_trace_modes() {
    run_matrix(
        "trace modes",
        &Matrix {
            cells: vec![("always", ""), ("on2", ""), ("on", "")],
            budgets: None,
            core_only: false,
            probe: None,
        },
    );
}

#[test]
fn identity_options() {
    run_matrix(
        "option variants",
        &Matrix {
            cells: vec![
                ("always", "-fold -cse"),
                ("always", "+exitstorm=2"),
                ("always", "+exitstorm=7"),
                ("always", "+randra=3"),
                ("always", "+membudget"),
                ("always", "-loop -fwd -abc -sink"),
                ("on2", "+exitstorm=3 +randra=9"),
            ],
            budgets: None,
            core_only: false,
            probe: None,
        },
    );
}

/// Every budget 1..=40 (and the usual ones) on the twin programs: the point at which a small per-stay budget runs
/// out moves through every instruction of every loop, so every landing label kind and every `rem` is hit.
#[test]
fn identity_budget_sweep() {
    run_matrix(
        "budget sweep",
        &Matrix {
            cells: vec![("always", ""), ("on2", "+exitstorm=5")],
            budgets: Some("1-40,3,17,200"),
            core_only: true,
            probe: None,
        },
    );
}

/// The mutant probe of `tr_mutants.rs`: a small family that passes without a mutant and fails (an identity gap, an
/// overshoot, a hung or crashed child) under every budget-accounting mutant (`refund_off_by_one`, `land_pre_mid_swap`,
/// `budget_no_store`, `budget_no_check`, `exit_refund_sign_flipped`). The children inherit `RUSTYNAIL_JIT_MUT`.
#[test]
fn mutant_probe() {
    run_matrix(
        "mutant probe",
        &Matrix {
            cells: vec![("always", ""), ("always", "+exitstorm=3"), ("on2", "")],
            budgets: None,
            core_only: false,
            probe: Some(&[
                "int_sum",
                "inline_exit",
                "inline_2",
                "float_forloop",
                "tc_acc",
                "nested_tables",
            ]),
        },
    );
}
