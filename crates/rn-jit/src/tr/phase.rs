//! BUNDLE-C: cheap per-phase compile-cost accounting (`rdtsc` pairs, a few per trace). The totals are printed with the
//! `RUSTYNAIL_JIT_VERBOSE=1` statistics (`[trace] phases ...`); they are what the compile-diet sub-passes were measured with.

use std::cell::Cell;

pub const NAMES: [&str; NPH] = [
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
    "front",
    "pipe_side",
    "res_env",
];
/// Number of phases.
pub const NPH: usize = 16;
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
/// Section 80.0: the passes before register allocation (`pipeline_front`: sinking, the root passes or the side subset, forwarding, presizing).
pub const FRONT: usize = 13;
/// Section 80.0: the pipeline of side traces alone (a subset of `PIPELINE`).
pub const PIPE_SIDE: usize = 14;
/// Section 80.0: the label tables and the environment between the exits and the assembler (`Res`, cells, counters, `AsmEnv`).
pub const RES_ENV: usize = 15;

thread_local! {
    static ACC: [Cell<u64>; NPH] = const { [const { Cell::new(0) }; NPH] };
    static CALLS: [Cell<u64>; NPH] = const { [const { Cell::new(0) }; NPH] };
    /// Section 80.0: the thread's user-mode instruction counter (`perf_event_open`); -2 = not opened yet, -1 = failed.
    static CTR_FD: Cell<i32> = const { Cell::new(-2) };
    static MAXPIPE: Cell<u64> = const { Cell::new(0) };
}

/// Section 43.2 (T2b): the timers run only when the statistics that print them are on (`RUSTYNAIL_JIT_VERBOSE >= 1`, [`super::diag::enabled`]):
/// four `rdtsc` per recorded bytecode were about 100 cycles of the recorder's ~1000 per step. A timer read while they are off is `0`, and
/// [`add`] ignores it.
#[inline(always)]
pub fn now() -> u64 {
    if ctr_mode() {
        return ctr_read();
    }
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
    if ROOT.with(Cell::get) {
        ACC_ROOT.with(|a| a[ph].set(a[ph].get() + d));
    }
    if ph == PIPELINE {
        MAXPIPE.with(|m| m.set(m.get().max(d)));
    }
}

/// Section 80.0 (`RUSTYNAIL_JIT_PHASECTR=1`): the phase timers count user-mode instructions of the thread (a `perf_event_open` counter read with
/// `read`, a few dozen user instructions per read) instead of TSC ticks, independently of `RUSTYNAIL_JIT_VERBOSE`; the totals are printed at exit
/// (`[trace] phases (Minstr/calls): ...`). Deterministic at a fixed seed: `scripts/compile-cost.sh` reads them.
static CTR_MODE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

#[inline(always)]
fn ctr_mode() -> bool {
    *CTR_MODE.get_or_init(|| {
        let on = std::env::var_os("RUSTYNAIL_JIT_PHASECTR").is_some_and(|v| v == "1");
        if on {
            extern "C" fn at_exit() {
                eprintln!("{}", report());
            }
            // SAFETY: registers a plain `extern "C"` function without captured state.
            unsafe { libc::atexit(at_exit) };
        }
        on
    })
}

#[cold]
fn ctr_open() -> i32 {
    #[cfg(all(target_os = "linux", not(miri)))]
    {
        // struct perf_event_attr (112 bytes, PERF_ATTR_SIZE_VER5): type HARDWARE (0), size, config INSTRUCTIONS (1), flags exclude_kernel
        // (bit 5) and exclude_hv (bit 6)
        let mut attr = [0u8; 112];
        attr[4..8].copy_from_slice(&112u32.to_ne_bytes());
        attr[8..16].copy_from_slice(&1u64.to_ne_bytes());
        let flags: u64 = (1 << 5) | (1 << 6);
        attr[40..48].copy_from_slice(&flags.to_ne_bytes());
        // SAFETY: the syscall reads `attr` (the 112 bytes it is told) and returns a descriptor or -1.
        let fd = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                attr.as_ptr(),
                0 as libc::pid_t,
                -1 as libc::c_int,
                -1 as libc::c_int,
                0 as libc::c_ulong,
            )
        };
        fd as i32
    }
    #[cfg(not(all(target_os = "linux", not(miri))))]
    -1
}

#[inline(always)]
fn ctr_read() -> u64 {
    let mut fd = CTR_FD.with(Cell::get);
    if fd == -2 {
        fd = ctr_open();
        CTR_FD.with(|c| c.set(fd));
    }
    if fd < 0 {
        return 0;
    }
    let mut v: u64 = 0;
    // SAFETY: reads 8 bytes into `v` from this thread's open counter descriptor.
    let n = unsafe { libc::read(fd, (&raw mut v).cast(), 8) };
    if n != 8 {
        return 0;
    }
    v | 1
}

pub fn report() -> String {
    let unit = if CTR_MODE.get().copied().unwrap_or(false) {
        "Minstr"
    } else {
        "Mtsc"
    };
    let mut o = format!("[trace] phases ({unit}/calls):");
    let sizes = format!(
        " sizes ins {} snaps {} snapents {} ops {} bytes {} stubops {} sharedops {} snapsame {} snapdup {} sidesame {} sideents {}",
        SIZES.with(|s| s[0].get()),
        SIZES.with(|s| s[1].get()),
        SIZES.with(|s| s[2].get()),
        SIZES.with(|s| s[3].get()),
        SIZES.with(|s| s[4].get()),
        SIZES.with(|s| s[5].get()),
        SIZES.with(|s| s[6].get()),
        SIZES.with(|s| s[7].get()),
        SIZES.with(|s| s[8].get()),
        SIZES.with(|s| s[9].get()),
        SIZES.with(|s| s[10].get())
    );
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
    o += &sizes;
    o += "\n[trace] root phases (Minstr):";
    ACC_ROOT.with(|a| {
        for i in 0..NAMES.len() {
            o += &format!(" {} {:.2}", NAMES[i], a[i].get() as f64 / 1e6);
        }
    });
    o
}

thread_local! {
    /// Section 80.5: a root trace's pipeline is running (its phases are added to `ACC_ROOT` too).
    static ROOT: Cell<bool> = const { Cell::new(false) };
    static ACC_ROOT: [Cell<u64>; NPH] = const { [const { Cell::new(0) }; NPH] };
}

/// Section 80.5: marks the pipeline running on this thread as a root trace's (`true`) or not.
pub fn set_root(on: bool) {
    ROOT.with(|c| c.set(on));
}

/// Section 80.0: sizes summed over the compiles (0 IR instructions, 1 snapshots, 2 snapshot entries, 3 micro-ops, 4 code bytes); counted only while
/// the timers run.
pub const SZ_INS: usize = 0;
pub const SZ_SNAPS: usize = 1;
pub const SZ_SNAPENTS: usize = 2;
pub const SZ_OPS: usize = 3;
pub const SZ_BYTES: usize = 4;
/// Micro-ops of the stubs and cold tails (`Cx::stubs_and_tail`).
pub const SZ_STUBOPS: usize = 5;
/// Micro-ops of the embedded shared exit routine.
pub const SZ_SHAREDOPS: usize = 6;
/// Section 80.5: snapshot entries that the previous snapshot holds too (same slot, ref and flags).
pub const SZ_SNAPSAME: usize = 7;
/// Section 80.5: snapshots whose entries and frames are those of the previous snapshot.
pub const SZ_SNAPDUP: usize = 8;
/// Section 80.5: the same two counts for side traces only (entries, all entries).
pub const SZ_SIDESAME: usize = 9;
pub const SZ_SIDEENTS: usize = 10;
const NSZ: usize = 11;

thread_local! {
    static SIZES: [Cell<u64>; NSZ] = const { [const { Cell::new(0) }; NSZ] };
}

/// Adds `n` to size counter `k` (only in the counter mode `RUSTYNAIL_JIT_PHASECTR=1`).
#[inline(always)]
pub fn size(k: usize, n: usize) {
    if CTR_MODE.get().copied().unwrap_or(false) {
        SIZES.with(|s| s[k].set(s[k].get() + n as u64));
    }
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
