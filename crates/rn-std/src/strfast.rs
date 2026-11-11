//! BUNDLE-B knob: the Rust string runtime fast paths, each switchable alone.
//!
//! `RUSTYNAIL_STR_FAST` is a bit mask read once per process (decimal or `0x` hex); unset means [`DEFAULT`].
//! Tests force bits with [`set_mask`]. Every piece is semantically transparent (the same results, errors and
//! `collectgarbage("count")` with every bit on or off).
use std::sync::atomic::{AtomicU32, Ordering};

/// S1 `pat1`: direct scan for one-item patterns (and `MaybeUninit` captures are always on: they are not observable).
pub const PAT1: u32 = 1;
/// S2 `fmtc`: cache of parsed `string.format` specifications.
pub const FMTC: u32 = 2;
/// S3 `patc`: compiled pattern cache for general patterns (`pattern/patc.rs`). With `PAT1` the one-item analysis comes
/// from the cached program instead of being redone per call.
pub const PATC: u32 = 4;
/// S4 `copy`: inline small copies/compares in the string heap chain (reserved, see docs/PERF.md).
pub const COPY: u32 = 8;

/// The process default (the one place that decides which pieces are on).
pub const DEFAULT: u32 = FMTC | PAT1 | PATC;

const UNINIT: u32 = u32::MAX;
static MASK: AtomicU32 = AtomicU32::new(UNINIT);

#[cold]
fn init() -> u32 {
    let m = match std::env::var("RUSTYNAIL_STR_FAST") {
        Ok(v) => {
            let v = v.trim();
            match v.strip_prefix("0x") {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => v.parse().ok(),
            }
            .unwrap_or(DEFAULT)
        }
        Err(_) => DEFAULT,
    };
    // keep the sentinel unreachable
    let m = m & 0xffff;
    let _ = MASK.compare_exchange(UNINIT, m, Ordering::Relaxed, Ordering::Relaxed);
    MASK.load(Ordering::Relaxed)
}

/// The active bit mask.
#[inline(always)]
pub fn mask() -> u32 {
    let m = MASK.load(Ordering::Relaxed);
    if m != UNINIT {
        m
    } else {
        init()
    }
}

/// Is the piece `bit` on?
#[inline(always)]
pub fn on(bit: u32) -> bool {
    mask() & bit != 0
}

/// Force the mask (tests).
pub fn set_mask(m: u32) {
    MASK.store(m & 0xffff, Ordering::Relaxed);
}
