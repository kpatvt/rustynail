//! Test-only interpreter hook (feature `t15_oracle`; OFF by default, the default build is byte-identical).
//!
//! The interpreter's dispatch loop calls the installed hook before it fetches every instruction of every Lua
//! frame (both interpreter copies). The tier-1.5 region-analysis oracle (`crates/rn-jit/tests/t15_oracle.rs`)
//! uses it as a VM-truth reference: the state *after* an instruction is the state seen at the next fetch of
//! the same frame, so a single callback gives "before" and "after" of every instruction (and of calls: the
//! caller's next fetch happens when the callee has returned).
use crate::func::Proto;
use crate::value::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

/// `hook(frame, proto, pc, base, maxstack)`: `frame` identifies the `CallInfo` (reused by later calls at the
/// same depth), `pc` is the index of the instruction about to run, `base` the address of register 0 (valid
/// during the call only; the hook may read and write the `maxstack` slots).
pub type Hook = fn(frame: usize, proto: *const Proto, pc: usize, base: *mut Value, maxstack: usize);

static HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install (or remove) the hook. Process-global.
pub fn set_hook(h: Option<Hook>) {
    HOOK.store(h.map_or(0, |f| f as usize), Ordering::SeqCst);
}

/// Instructions the interpreter executed WITHOUT fetching them (no hook call): the `TFORLOOP` that `TFORCALL` runs
/// inline after its call returned, and the `TFORCALL` + `TFORLOOP` that `TFORPREP` runs inline. They are counted when
/// they execute, so a `TFORCALL` whose call raises an error (or yields) never counts the `TFORLOOP` it did not reach,
/// and a Lua iterator's own fetches are ordered before the `TFORLOOP` that follows them.
static UNFETCHED: AtomicUsize = AtomicUsize::new(0);

/// The number of unfetched instructions executed since `reset_unfetched`.
pub fn unfetched() -> u64 {
    UNFETCHED.load(Ordering::SeqCst) as u64
}

/// Reset the unfetched-instruction counter.
pub fn reset_unfetched() {
    UNFETCHED.store(0, Ordering::SeqCst);
}

/// One more instruction executed without a fetch.
#[inline]
pub(crate) fn note_unfetched() {
    UNFETCHED.fetch_add(1, Ordering::Relaxed);
}

/// Is a hook installed?
#[inline]
pub(crate) fn active() -> bool {
    HOOK.load(Ordering::Relaxed) != 0
}

/// Call the hook (if still installed).
#[inline(never)]
pub(crate) fn fire(
    frame: usize,
    proto: *const Proto,
    pc: usize,
    base: *mut Value,
    maxstack: usize,
) {
    let h = HOOK.load(Ordering::Relaxed);
    if h != 0 {
        // SAFETY: only `set_hook` stores a non-zero value, and it stores a `Hook`.
        let f: Hook = unsafe { std::mem::transmute::<usize, Hook>(h) };
        f(frame, proto, pc, base, maxstack);
    }
}
