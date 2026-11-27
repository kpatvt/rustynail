//! Section 76.4 (`carena`, docs/JIT_TRACE.md section 76.4): the compile arena. While a JIT compile runs (a recorder step and the trace
//! pipeline it may start, a tier-1 compile, a deferred exit stub: [`enter`] / [`leave`]) and the arena is switched on ([`set_enabled`]),
//! the process allocator ([`CArena`], registered by `rnlua` and by the `tr_core` test binary) serves the small allocations of that thread
//! from 16 KiB chunks of one reserved address range by bumping a pointer, and frees by counting: every chunk counts its live blocks, and a
//! chunk whose count drops to zero is recycled (a per-thread free list). A `realloc` of the newest block of the current chunk grows or
//! shrinks in place (the common `Vec` growth). Everything else (the arena off, outside a compile, blocks over [`MAX_BLOCK`], the range used
//! up) goes to the system allocator; a free decides by address range, so a block may be freed in any mode and on any thread.
//!
//! The arena changes no result: it only decides where the bytes of a block live. Blocks of a compile that outlive it (installed trace
//! descriptors, recorder state between steps) keep their chunk alive until they are freed.
//!
//! `unsafe` is allowed in this module; every block carries a `// SAFETY:` comment.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// Bytes of one chunk (a power of two, aligned: a block's chunk is its address rounded down).
pub const CHUNK: usize = 16 * 1024;
/// Bytes of the reserved range (one system block; its pages are committed when first touched).
pub const RANGE: usize = 256 << 20;
/// Largest block served by the arena.
pub const MAX_BLOCK: usize = 2 * 1024;
/// Bytes at the start of a chunk: the chunk header.
const HDR: usize = 64;

static ENABLED: AtomicBool = AtomicBool::new(false);
static BASE: AtomicUsize = AtomicUsize::new(0);
/// The next never-used chunk of the range (offset).
static FRESH: AtomicUsize = AtomicUsize::new(0);
/// Statistics: chunks recycled.
static RECYCLED: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "jit_trace_mut")]
static MUT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Test-only mutants of the compile arena (feature `jit_trace_mut`), a bit mask: `1` `carena_recycle_live` (a chunk is recycled while one block
/// of it is still live), `2` `carena_grow_any` (`realloc` grows any block of the current chunk in place, not only the newest).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_carena(bits: u8) {
    MUT.store(bits, Ordering::Relaxed);
}

#[inline(always)]
fn mutant(bit: u8) -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT.load(Ordering::Relaxed) & bit != 0;
    #[cfg(not(feature = "jit_trace_mut"))]
    {
        let _ = bit;
        false
    }
}

/// The header of a chunk. Only its owner thread allocates from a chunk, so the owner counts the blocks handed out and the blocks it freed itself
/// in `live` (plain memory, owner thread only); a block freed by another thread is counted in `remote`. The chunk is empty when `live == remote`,
/// which only the owner tests (a remote free that empties a chunk leaves it unused: a leak of one chunk, never reached in `rnlua`).
#[repr(C)]
struct Chunk {
    /// The owner thread (the address of its [`MARK`]).
    owner: usize,
    /// Blocks handed out minus blocks the owner freed, plus one while it is the current chunk of its owner.
    live: Cell<u32>,
    /// Blocks of the chunk freed by other threads.
    remote: AtomicU32,
    /// The next chunk of the owner's free list (0 = none), owner thread only.
    next: usize,
}

thread_local! {
    /// Nesting depth of compiles on this thread.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The arena serves this thread now (on, inside a compile): set by [`enter`] / [`leave`].
    static ACT: Cell<bool> = const { Cell::new(false) };
    /// The current chunk of this thread (address, 0 = none), its bump pointer, its newest block (for in-place growth, 0 = none).
    static CUR: Cell<usize> = const { Cell::new(0) };
    static TOP: Cell<usize> = const { Cell::new(0) };
    static LAST: Cell<usize> = const { Cell::new(0) };
    /// The recycled chunks of this thread (a list through `Chunk::next`).
    static FREE: Cell<usize> = const { Cell::new(0) };
    /// This thread's identity (its address).
    static MARK: u8 = const { 0 };
    /// Statistics: blocks served on this thread.
    static SERVED: Cell<usize> = const { Cell::new(0) };
}

/// Switches the arena on or off for the allocations from now on (the knob `carena`). The first switch-on reserves the range.
pub fn set_enabled(on: bool) {
    if on && BASE.load(Ordering::Acquire) == 0 {
        // one aligned system block for the whole range: the system allocator maps it (pages are committed when touched) and never returns
        // another block inside it while it is live (it is never freed)
        let Ok(l) = Layout::from_size_align(RANGE, CHUNK) else {
            return;
        };
        // SAFETY: a non-zero size; the result is checked.
        let p = unsafe { System.alloc(l) } as usize;
        if p == 0 {
            return;
        }
        if BASE
            .compare_exchange(0, p, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // another thread reserved it first
            // SAFETY: our own block with this layout, unused.
            unsafe { System.dealloc(p as *mut u8, l) };
        }
    }
    ENABLED.store(on && BASE.load(Ordering::Acquire) != 0, Ordering::Release);
}

/// Is the arena switched on?
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// A compile starts on this thread.
#[inline]
pub fn enter() {
    let n = DEPTH.with(|d| {
        d.set(d.get() + 1);
        d.get()
    });
    ACT.with(|a| a.set(n != 0 && ENABLED.load(Ordering::Relaxed)));
}

/// The compile of the matching [`enter`] ends.
#[inline]
pub fn leave() {
    let n = DEPTH.with(|d| {
        d.set(d.get().saturating_sub(1));
        d.get()
    });
    ACT.with(|a| a.set(n != 0 && ENABLED.load(Ordering::Relaxed)));
}

/// Blocks served by the arena on this thread / chunks recycled / chunks of the range ever taken (statistics).
pub fn stats() -> (usize, usize, usize) {
    (
        SERVED.with(|c| c.get()),
        RECYCLED.load(Ordering::Relaxed),
        FRESH.load(Ordering::Relaxed) / CHUNK,
    )
}

#[inline]
fn in_range(p: usize) -> bool {
    let b = BASE.load(Ordering::Relaxed);
    b != 0 && p.wrapping_sub(b) < RANGE
}

#[inline]
fn mark() -> usize {
    MARK.with(|m| m as *const u8 as usize)
}

/// Does this thread allocate from the arena now?
#[inline]
fn active() -> bool {
    ACT.with(|a| a.get())
}

/// Drops one live count of the chunk at `c`: on its owner thread the last one recycles it onto the owner's free list, on another thread it is
/// counted in `remote` (see [`Chunk`]).
///
/// # Safety
/// `c` is the address of a chunk of the range whose count includes the one dropped here.
unsafe fn unref(c: usize) {
    // SAFETY: `c` is a chunk of the range, its header was written when it was taken (caller contract).
    let h = unsafe { &*(c as *const Chunk) };
    if h.owner != mark() {
        h.remote.fetch_add(1, Ordering::AcqRel);
        return;
    }
    // (owner thread: `live` and `next` are ours)
    let n = h.live.get() - 1;
    h.live.set(n);
    let remote = h.remote.load(Ordering::Acquire);
    if n == remote || (mutant(1) && n == remote + 1) {
        RECYCLED.fetch_add(1, Ordering::Relaxed);
        FREE.with(|f| {
            // SAFETY: the chunk has no live block and is not current: its owner may rewrite its header.
            unsafe { (*(c as *mut Chunk)).next = f.get() };
            f.set(c);
        });
    }
}

/// A new current chunk for this thread, or `None` when the range is used up.
fn new_chunk() -> Option<usize> {
    let c = FREE.with(|f| {
        let c = f.get();
        if c != 0 {
            // SAFETY: a recycled chunk of this thread: its header is ours.
            f.set(unsafe { (*(c as *const Chunk)).next });
        }
        c
    });
    let c = if c != 0 {
        c
    } else {
        let off = FRESH.fetch_add(CHUNK, Ordering::Relaxed);
        if off + CHUNK > RANGE {
            return None;
        }
        BASE.load(Ordering::Relaxed) + off
    };
    // SAFETY: the chunk is unused (fresh, or recycled with no live block); the range is mapped read-write.
    unsafe {
        (c as *mut Chunk).write(Chunk {
            owner: mark(),
            live: Cell::new(1),
            remote: AtomicU32::new(0),
            next: 0,
        })
    };
    Some(c)
}

/// Bump allocation from this thread's current chunk (the arena active), or null (too big, range used up).
fn bump(layout: Layout) -> *mut u8 {
    let size = layout.size().max(1);
    let align = layout.align().max(16);
    if size > MAX_BLOCK || align > 4096 {
        return std::ptr::null_mut();
    }
    let mut c = CUR.with(|c| c.get());
    loop {
        if c != 0 {
            let p = (TOP.with(|t| t.get()) + align - 1) & !(align - 1);
            if p + size <= c + CHUNK {
                TOP.with(|t| t.set(p + size));
                LAST.with(|l| l.set(p));
                // SAFETY: `c` is the current chunk of this thread, its header is live; `live` is the owner's.
                let h = unsafe { &*(c as *const Chunk) };
                h.live.set(h.live.get() + 1);
                SERVED.with(|n| n.set(n.get() + 1));
                return p as *mut u8;
            }
            // retire the current chunk (drop its "current" count)
            CUR.with(|x| x.set(0));
            LAST.with(|l| l.set(0));
            // SAFETY: the current chunk holds the "current" count.
            unsafe { unref(c) };
        }
        c = match new_chunk() {
            Some(n) => n,
            None => return std::ptr::null_mut(),
        };
        CUR.with(|x| x.set(c));
        TOP.with(|t| t.set(c + HDR));
    }
}

/// The process allocator with the compile arena (see the module documentation).
pub struct CArena;

// SAFETY: blocks come either from `System` (and go back to it: decided by address range, and `System` never returns a block inside the reserved
// range, a live system block of its own) or from a chunk of the range, aligned and sized as the layout asks and disjoint from every other live
// block: the bump pointer only advances within a chunk (it moves back only over the newest block when that block is freed), a block grows in
// place only when it is the newest one of the chunk, and a chunk is reused only after its live count, which counts every block handed out from
// it, dropped to zero.
unsafe impl GlobalAlloc for CArena {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if active() {
            let p = bump(layout);
            if !p.is_null() {
                return p;
            }
        }
        // SAFETY: forwarded with the caller's layout.
        unsafe { System.alloc(layout) }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let p = ptr as usize;
        if in_range(p) {
            if LAST.with(|l| l.get()) == p {
                // the newest block of this thread's current chunk: give its bytes back to the bump pointer
                TOP.with(|t| t.set(p));
                LAST.with(|l| l.set(0));
            }
            // SAFETY: a block of the range: its chunk's count includes it.
            unsafe { unref(p & !(CHUNK - 1)) };
            return;
        }
        // SAFETY: not an arena block: it came from `System` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = ptr as usize;
        if in_range(p) {
            // in place: the newest block of this thread's current chunk, if the new size fits the chunk
            let c = CUR.with(|c| c.get());
            if (LAST.with(|l| l.get()) == p || (mutant(2) && c == p & !(CHUNK - 1) && c != 0))
                && p + new_size <= c + CHUNK
                && new_size <= MAX_BLOCK
            {
                TOP.with(|t| t.set(p + new_size.max(1)));
                LAST.with(|l| l.set(p));
                return ptr;
            }
            // SAFETY: a valid layout (the caller guarantees `new_size` rounded to this alignment does not overflow).
            let nl = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
            // SAFETY: as `alloc`.
            let q = unsafe { self.alloc(nl) };
            if !q.is_null() {
                // SAFETY: both blocks are live and disjoint, each at least `min(old, new)` bytes.
                unsafe { std::ptr::copy_nonoverlapping(ptr, q, layout.size().min(new_size)) };
                // SAFETY: the old block, freed once.
                unsafe { self.dealloc(ptr, layout) };
            }
            return q;
        }
        // (a system block growing inside a compile stays a system block: no copy into the arena)
        // SAFETY: a system block with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
