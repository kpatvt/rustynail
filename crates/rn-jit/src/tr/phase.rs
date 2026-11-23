//! BUNDLE-C: cheap per-phase compile-cost accounting (`rdtsc` pairs, a few per trace). The totals are printed with the
//! `RUSTYNAIL_JIT_VERBOSE=1` statistics (`[trace] phases ...`); they are what the compile-diet sub-passes were measured with.

use std::cell::Cell;

pub const NAMES: [&str; 13] = [
    "rec_step",
    "lockstep",
    "pass_sink",
    "pass_other",
    "ra",
    "exits",
    "asm",
    "x86",
    "install",
    "hash",
    "pipeline",
    "tier1",
    "lazystub",
];
pub const REC_STEP: usize = 0;
pub const LOCKSTEP: usize = 1;
pub const PASS_SINK: usize = 2;
pub const PASS_OTHER: usize = 3;
pub const RA: usize = 4;
pub const EXITS: usize = 5;
pub const ASM: usize = 6;
pub const X86: usize = 7;
pub const INSTALL: usize = 8;
pub const HASH: usize = 9;
pub const PIPELINE: usize = 10;
pub const TIER1: usize = 11;
/// S1: assembling and placing the deferred exit stubs (`exit::defer_build`).
pub const LAZYSTUB: usize = 12;

thread_local! {
    static ACC: [Cell<u64>; 13] = const { [const { Cell::new(0) }; 13] };
    static CALLS: [Cell<u64>; 13] = const { [const { Cell::new(0) }; 13] };
    static MAXPIPE: Cell<u64> = const { Cell::new(0) };
}

/// Section 43.2 (T2b): the timers run only when the statistics that print them are on (`RUSTYNAIL_JIT_VERBOSE >= 1`, [`super::diag::enabled`]):
/// four `rdtsc` per recorded bytecode were about 100 cycles of the recorder's ~1000 per step. A timer read while they are off is `0`, and
/// [`add`] ignores it.
#[inline(always)]
pub fn now() -> u64 {
    if !super::diag::enabled() {
        return 0;
    }
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    // SAFETY: rdtsc has no preconditions.
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
    // (Miri cannot call the intrinsic: the compile-cost phases read 0 there)
    #[cfg(any(not(target_arch = "x86_64"), miri))]
    0
}

#[inline(always)]
pub fn add(ph: usize, t0: u64) {
    if t0 == 0 {
        return;
    }
    let d = now().wrapping_sub(t0);
    ACC.with(|a| a[ph].set(a[ph].get() + d));
    CALLS.with(|a| a[ph].set(a[ph].get() + 1));
    if ph == PIPELINE {
        MAXPIPE.with(|m| m.set(m.get().max(d)));
    }
}

pub fn report() -> String {
    let mut o = String::from("[trace] phases (Mtsc/calls):");
    ACC.with(|a| {
        CALLS.with(|c| {
            for i in 0..NAMES.len() {
                o += &format!(
                    " {} {:.2}/{}",
                    NAMES[i],
                    a[i].get() as f64 / 1e6,
                    c[i].get()
                );
            }
        })
    });
    o += &format!(" max_pipeline {}", MAXPIPE.with(|m| m.get()));
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Section 43.2 (T2b): a timer that was read while the statistics were off (`0`) is not added.
    #[test]
    fn a_zero_timer_is_ignored() {
        let (a, c) = (ACC.with(|a| a[RA].get()), CALLS.with(|c| c[RA].get()));
        add(RA, 0);
        assert_eq!(ACC.with(|a| a[RA].get()), a);
        assert_eq!(CALLS.with(|c| c[RA].get()), c);
    }
}
