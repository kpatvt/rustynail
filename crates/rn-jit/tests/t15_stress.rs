//! Tier 1.5 verification stress (task T8; docs/JIT_TIER15.md section 22): the whole twin corpus of `t15_core.rs`
//! (fixed, mid-run mutation, random integer / float, table, cross-class and intrinsic programs, about 300 programs) is
//! run under the verification options and compared with the jit-off twin:
//!
//! * `+shadow` (tier-1 emulation of every pure region block, aborts on a mismatch),
//! * `+exitstorm=N`, N in {1, 2, 7, 61} (pseudo-guards failing every N-th execution),
//! * `+randra=SEED`, `+flushall`, `+verify` and combinations,
//! * with and without a warm-up profile (`T15=always` and `T15=on` with a small threshold),
//! * under a few count budgets,
//!
//! and the executed-instruction count (`Stats::insns`, C1) must equal the one of the run without regions.
//!
//! Heavy: not part of a plain `cargo test` (`test = false` in Cargo.toml). Run it with
//! `cargo test -p rn-jit --features jit --test t15_stress`; also under `RUSTYNAIL_GC_STRESS=1|2` and
//! `RUSTYNAIL_JIT_STRESS=stackmove` (see docs/JIT_TESTING.md).
#![cfg(feature = "jit")]
#![allow(dead_code)]

#[path = "t15_core.rs"]
mod core_tests;

use core_tests::{cfg, cfg_prof, run_with, stress_corpus};
use rn_jit::t15::runtime::{T15Config, T15Mode};
use rn_jit::Mode;

/// Runs the corpus with the option string `o` (`+force` is added) and checks output and instruction counts.
fn stress(o: &str) {
    let (checked0, _) = rn_jit::t15::verify::shadow_counts();
    stress_inner(o);
    let (checked1, inconclusive) = rn_jit::t15::verify::shadow_counts();
    if o.contains("+shadow") && !rn_core::jit_layout::LUA54 {
        eprintln!(
            "opt `{o}`: shadow blocks checked so far {checked1}, inconclusive {inconclusive}"
        );
        assert!(
            checked1 > checked0 + 100,
            "opt `{o}`: the shadow verification checked almost nothing ({checked0} -> {checked1})"
        );
    }
}

fn stress_inner(o: &str) {
    for (name, src) in stress_corpus() {
        let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        let (out0, s0) = run_with(&src, Mode::Always, T15Config::for_mode(T15Mode::Off), None);
        assert_eq!(want, out0, "{name}: jit-off twin");
        let (got, s1) = run_with(&src, Mode::Always, cfg(o), None);
        assert_eq!(want, got, "{name}: opt `{o}` (T15=always) differs");
        assert_eq!(
            s0.insns, s1.insns,
            "{name}: instruction count differs, opt `{o}` (T15=always)"
        );
        for hot in [2, 9] {
            let (got, _) = run_with(&src, Mode::Always, cfg_prof(o, hot), None);
            assert_eq!(want, got, "{name}: opt `{o}` (profiled, hot {hot}) differs");
        }
    }
}

/// Like [`stress`] under count budgets (the budget exits and refunds interleave with the pseudo-guard exits).
fn stress_budget(o: &str) {
    for (name, src) in stress_corpus() {
        for b in [3, 37] {
            let (out0, s0) = run_with(
                &src,
                Mode::Always,
                T15Config::for_mode(T15Mode::Off),
                Some(b),
            );
            let (got, s1) = run_with(&src, Mode::Always, cfg(o), Some(b));
            assert_eq!(out0, got, "{name}: opt `{o}`, budget {b} differs");
            assert_eq!(
                s0.insns, s1.insns,
                "{name}: instruction count differs, opt `{o}`, budget {b}"
            );
        }
    }
}

macro_rules! stress_tests {
    ($($name:ident => $opt:expr,)*) => {
        $(#[test] fn $name() { stress($opt); })*
    };
}

stress_tests! {
    s_shadow => "+shadow",
    s_shadow_ra => "+shadow,-ra",
    s_shadow_rand => "+shadow,+randra=3",
    s_shadow_rand2 => "+shadow,+randra=17",
    s_shadow_flushall => "+shadow,+flushall",
    s_shadow_verify => "+shadow,+verify",
    s_storm1 => "+exitstorm=1",
    s_storm2 => "+exitstorm=2",
    s_storm7 => "+exitstorm=7",
    s_storm61 => "+exitstorm=61",
    s_storm1_rand => "+exitstorm=1,+randra=5",
    s_storm2_rand => "+exitstorm=2,+randra=9",
    s_storm7_rand => "+exitstorm=7,+randra=2",
    s_storm7_verify => "+exitstorm=7,+verify",
    s_storm2_verify => "+exitstorm=2,+verify,+flushall",
    s_storm7_shadow => "+exitstorm=7,+shadow",
    s_storm2_shadow_rand => "+exitstorm=2,+shadow,+randra=6",
    s_randra_a => "+randra=101",
    s_randra_b => "+randra=202",
    s_randra_c => "+randra=303",
    s_flushall => "+flushall",
    s_verify => "+verify",
    s_verify_flush_rand => "+verify,+flushall,+randra=8",
}

#[test]
fn s_budget_storm() {
    for o in ["+exitstorm=1", "+exitstorm=7", "+exitstorm=2,+shadow"] {
        stress_budget(o);
    }
}

#[test]
fn s_budget_shadow() {
    for o in ["+shadow,+randra=4", "+flushall,+exitstorm=61"] {
        stress_budget(o);
    }
}
