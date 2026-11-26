//! BUNDLE-B knob: the Rust string runtime fast paths (and, since section 59, two `table` library ones), each switchable alone.
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
/// R1 `memfind`: plain substring search 16 start positions at a time, filtered by the needle's first and last bytes (SSE2;
/// docs/STUDY_RUNTIME_GAP.md). The scalar first-byte search runs with the bit off, for needles of one byte and for haystacks
/// with fewer than 16 start positions.
pub const MEMFIND: u32 = 16;

/// M1 `tmovepre` (docs/JIT_TRACE.md section 59): `table.move(a, f, e, 1, b)` into an empty table `b` without a metatable sizes `b`'s array part
/// first when every value of `a[f..e]` is non-nil and in `a`'s array part (`TableRef::move_presize`: the size the element copy would grow to),
/// so that the bulk copy applies.
pub const TMOVEPRE: u32 = 32;
/// R1 `tpop` (docs/JIT_TRACE.md section 59): `table.remove(t)` (one argument) of a table without a metatable: raw border, raw read, raw store
/// of nil, without the generic argument checks.
pub const TPOP: u32 = 64;

/// S6 `gmplus` (docs/JIT_TRACE.md section 64): the `string.gmatch` iterator of a pattern that is exactly `%x+` (one class escape and
/// `+`) scans the class directly, without the pattern matcher's state, start filter, match dispatch and capture protocol.
pub const GMPLUS: u32 = 128;

/// The process default (the one place that decides which pieces are on).
pub const DEFAULT: u32 = FMTC | PAT1 | PATC | MEMFIND | TMOVEPRE | TPOP | GMPLUS;

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
