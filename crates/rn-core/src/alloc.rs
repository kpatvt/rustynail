//! Fallible allocation layer (port of `lmem.c` semantics; ADR-2).
//!
//! All runtime allocation goes through an [`Allocator`] with `lua_Alloc` semantics. [`Mem`] wraps one,
//! keeps the byte accounting (`GCdebt` input for the collector) and an optional hard limit (used by
//! tests to provoke `ErrMem`). [`RawBuf`] is a fallible, manually-managed buffer of `Copy` elements.
//!
//! `unsafe` is allowed in this module; every block carries a `// SAFETY:` comment.
use crate::status::{LuaResult, Status};
use std::alloc::Layout;
use std::ptr::NonNull;

/// Alignment of every block handed out by the system allocator (and required of custom allocators).
pub const BLOCK_ALIGN: usize = 16;

/// `lua_Alloc` without the `ud` pointer (the implementor owns its state).
///
/// `realloc(ptr, old, new)`:
/// * `new == 0`: frees `ptr` (if non-null; `old` is its size) and returns null;
/// * `ptr` null: allocates `new` bytes;
/// * otherwise resizes the `old`-byte block to `new` bytes.
///
/// Returns null on failure, leaving any existing block untouched. Blocks must be aligned to
/// [`BLOCK_ALIGN`].
pub trait Allocator {
    /// # Safety
    /// `ptr` must be null or a live block previously returned by this allocator with size exactly `old`.
    unsafe fn realloc(&mut self, ptr: *mut u8, old: usize, new: usize) -> *mut u8;

    /// A new block of `new` (> 0) bytes, or null. P-J: the common case of `realloc(null, 0, new)` as its own entry point so an
    /// allocator can skip the resize branches (the default forwards).
    ///
    /// # Safety
    /// As for [`Allocator::realloc`].
    #[inline]
    unsafe fn alloc_block(&mut self, new: usize) -> *mut u8 {
        // SAFETY: the "new block" case of `realloc`.
        unsafe { self.realloc(std::ptr::null_mut(), 0, new) }
    }

    /// Frees the live `old`-byte block `ptr` (`old` > 0).
    ///
    /// # Safety
    /// As for [`Allocator::realloc`] with `new == 0`.
    #[inline]
    unsafe fn free_block(&mut self, ptr: *mut u8, old: usize) {
        // SAFETY: forwarded caller contract.
        unsafe { self.realloc(ptr, old, 0) };
    }
}

/// The default allocator: the process global allocator.
#[derive(Default, Debug, Clone, Copy)]
pub struct SystemAllocator;

impl Allocator for SystemAllocator {
    unsafe fn realloc(&mut self, ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
        if new == 0 {
            if !ptr.is_null() && old != 0 {
                // SAFETY: caller guarantees (ptr, old) describes a live block of this allocator, which
                // was created with this exact layout.
                unsafe {
                    std::alloc::dealloc(ptr, Layout::from_size_align_unchecked(old, BLOCK_ALIGN))
                };
            }
            return std::ptr::null_mut();
        }
        let Ok(new_layout) = Layout::from_size_align(new, BLOCK_ALIGN) else {
            return std::ptr::null_mut();
        };
        if ptr.is_null() || old == 0 {
            // SAFETY: `new_layout` has non-zero size.
            unsafe { std::alloc::alloc(new_layout) }
        } else {
            // SAFETY: (ptr, old) is a live block allocated with alignment BLOCK_ALIGN; `new` is non-zero
            // and `new_layout` proved it does not overflow `isize` when rounded to the alignment.
            unsafe {
                std::alloc::realloc(
                    ptr,
                    Layout::from_size_align_unchecked(old, BLOCK_ALIGN),
                    new,
                )
            }
        }
    }
}

/// Largest block size served by the [`PoolAllocator`] free lists.
const POOL_MAX: usize = 512;
/// Number of 16-byte size classes (`1..=POOL_MAX / 16`; class 0 is unused).
const POOL_CLASSES: usize = POOL_MAX / BLOCK_ALIGN + 1;
/// Size of the arenas small blocks are carved from.
const ARENA_SIZE: usize = 256 * 1024;

/// The default allocator of a state: segregated free lists (16-byte size classes up to
/// `POOL_MAX` bytes) carved from 256 KB arenas, larger blocks from the system allocator.
///
/// The collector frees objects with their exact size (`lua_Alloc` contract), so a block needs no
/// header and a free is a push on its class list: cheaper and denser than `malloc` (which adds an
/// 8-byte header and rounds to 16) for the many small objects of a Lua heap. The arenas are
/// released when the allocator (the state) is dropped. Not shared between states.
pub struct PoolAllocator {
    free: [*mut FreeBlock; POOL_CLASSES],
    /// Bump region of the current arena.
    bump: *mut u8,
    bump_end: *mut u8,
    arenas: Vec<*mut u8>,
}

struct FreeBlock {
    next: *mut FreeBlock,
}

impl Default for PoolAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolAllocator {
    pub fn new() -> Self {
        PoolAllocator {
            free: [std::ptr::null_mut(); POOL_CLASSES],
            bump: std::ptr::null_mut(),
            bump_end: std::ptr::null_mut(),
            arenas: Vec::new(),
        }
    }

    #[inline(always)]
    fn class(size: usize) -> usize {
        size.div_ceil(BLOCK_ALIGN)
    }

    /// Allocate a block of size class `c` (`1..POOL_CLASSES`).
    #[inline(always)]
    fn alloc_small(&mut self, c: usize) -> *mut u8 {
        // SAFETY: `c < POOL_CLASSES` (callers check `size <= POOL_MAX`).
        let head = unsafe { self.free.get_unchecked_mut(c) };
        let b = *head;
        if !b.is_null() {
            // SAFETY: `b` is a free block of this class: its first word is the list link.
            *head = unsafe { (*b).next };
            return b.cast();
        }
        let sz = c * BLOCK_ALIGN;
        if (self.bump_end as usize) - (self.bump as usize) >= sz {
            let p = self.bump;
            // SAFETY: `p + sz <= bump_end` (checked above), inside the current arena.
            self.bump = unsafe { p.add(sz) };
            return p;
        }
        self.refill(sz)
    }

    #[cold]
    #[inline(never)]
    fn refill(&mut self, sz: usize) -> *mut u8 {
        // the unused tail of the old arena goes to the free lists
        // SAFETY: the tail `[bump, bump_end)` is unused arena memory, a multiple of 16 bytes.
        unsafe {
            let mut rest = (self.bump_end as usize) - (self.bump as usize);
            while rest >= BLOCK_ALIGN {
                let c = (rest / BLOCK_ALIGN).min(POOL_CLASSES - 1);
                let b = self.bump.cast::<FreeBlock>();
                (*b).next = self.free[c];
                self.free[c] = b;
                self.bump = self.bump.add(c * BLOCK_ALIGN);
                rest -= c * BLOCK_ALIGN;
            }
        }
        if self.arenas.try_reserve(1).is_err() {
            return std::ptr::null_mut();
        }
        // SAFETY: non-zero size.
        let a = unsafe {
            std::alloc::alloc(Layout::from_size_align_unchecked(ARENA_SIZE, BLOCK_ALIGN))
        };
        if a.is_null() {
            return a;
        }
        self.arenas.push(a);
        // SAFETY: `sz <= POOL_MAX < ARENA_SIZE`.
        unsafe {
            self.bump = a.add(sz);
            self.bump_end = a.add(ARENA_SIZE);
        }
        a
    }

    #[inline(always)]
    fn free_small(&mut self, p: *mut u8, c: usize) {
        let b = p.cast::<FreeBlock>();
        // SAFETY: `p` is a dead block of class `c` (at least 16 bytes, 16-aligned).
        unsafe {
            (*b).next = self.free[c];
            *self.free.get_unchecked_mut(c) = b;
        }
    }
}

impl Drop for PoolAllocator {
    fn drop(&mut self) {
        for &a in &self.arenas {
            // SAFETY: every arena was allocated in `refill` with this layout.
            unsafe {
                std::alloc::dealloc(
                    a,
                    Layout::from_size_align_unchecked(ARENA_SIZE, BLOCK_ALIGN),
                )
            };
        }
    }
}

impl Allocator for PoolAllocator {
    #[inline]
    unsafe fn alloc_block(&mut self, new: usize) -> *mut u8 {
        if new <= POOL_MAX {
            self.alloc_small(Self::class(new))
        } else {
            // SAFETY: a fresh system block.
            unsafe { SystemAllocator.realloc(std::ptr::null_mut(), 0, new) }
        }
    }

    #[inline]
    unsafe fn free_block(&mut self, ptr: *mut u8, old: usize) {
        if old <= POOL_MAX {
            self.free_small(ptr, Self::class(old));
        } else {
            // SAFETY: forwarded contract: a live system block of `old` bytes.
            unsafe { SystemAllocator.realloc(ptr, old, 0) };
        }
    }

    #[inline]
    unsafe fn realloc(&mut self, ptr: *mut u8, old: usize, new: usize) -> *mut u8 {
        let old = if ptr.is_null() { 0 } else { old };
        let small_old = old != 0 && old <= POOL_MAX;
        if new == 0 {
            if small_old {
                self.free_small(ptr, Self::class(old));
            } else {
                // SAFETY: forwarded contract (large or null block).
                unsafe { SystemAllocator.realloc(ptr, old, 0) };
            }
            return std::ptr::null_mut();
        }
        if new <= POOL_MAX {
            let c = Self::class(new);
            if small_old && Self::class(old) == c {
                return ptr;
            }
            let p = self.alloc_small(c);
            if !p.is_null() && old != 0 {
                // SAFETY: both blocks are live and distinct; `min(old, new)` bytes fit in both.
                unsafe {
                    std::ptr::copy_nonoverlapping(ptr, p, old.min(new));
                    self.realloc(ptr, old, 0);
                }
            }
            return p;
        }
        if old == 0 || !small_old {
            // SAFETY: forwarded contract: (ptr, old) is a system block or null.
            return unsafe { SystemAllocator.realloc(ptr, old, new) };
        }
        // small -> large
        // SAFETY: fresh system block.
        let p = unsafe { SystemAllocator.realloc(std::ptr::null_mut(), 0, new) };
        if !p.is_null() {
            // SAFETY: both live; `old < new`.
            unsafe { std::ptr::copy_nonoverlapping(ptr, p, old) };
            self.free_small(ptr, Self::class(old));
        }
        p
    }
}

/// Allocator + accounting. One per `Heap`.
pub struct Mem {
    alloc: Box<dyn Allocator>,
    total: usize,
    /// Bytes of "buffer" blocks that count toward the memory limit but not toward the collector's
    /// debt (the reference builds big strings in `luaL_Buffer` boxes outside `luaM_`).
    ext: usize,
    limit: usize,
    /// J5 allocation sinking (TR29.4): bytes charged for sunk allocations that never happened. They count for the
    /// collector (`total_bytes` = `total + virt`) but not for the memory limit (P-L: kept apart from `total`, so the trace
    /// code charges one cell).
    #[cfg(feature = "jit_trace")]
    virt: usize,
}

impl Mem {
    pub fn new(alloc: Box<dyn Allocator>) -> Self {
        Mem {
            alloc,
            total: 0,
            ext: 0,
            limit: usize::MAX,
            #[cfg(feature = "jit_trace")]
            virt: 0,
        }
    }

    /// The limit the real (non-virtual) bytes are checked against.
    #[inline(always)]
    fn lim(&self) -> usize {
        // (P-L: `total` holds the real bytes only, the virtual ones of J5 sinking live in `virt`: the limit needs no
        // adjustment)
        self.limit
    }

    /// Charges `n` bytes of virtual allocation (a sunk table that was never allocated) to the collector-visible total.
    #[cfg(feature = "jit_trace")]
    pub fn charge_virtual(&mut self, n: usize) {
        self.virt = self.virt.saturating_add(n);
    }

    /// Credits back up to `n` virtual bytes.
    #[cfg(feature = "jit_trace")]
    pub fn uncharge_virtual(&mut self, n: usize) {
        let n = n.min(self.virt);
        self.virt -= n;
    }

    /// Virtual bytes currently charged.
    #[cfg(feature = "jit_trace")]
    pub fn virtual_bytes(&self) -> usize {
        self.virt
    }

    /// Address of the `virt` counter (the trace code adds to it in place; P-L: one cell, the collector-visible total is
    /// `total + virt`).
    #[cfg(feature = "jit_trace")]
    pub fn virtual_cell(&mut self) -> *mut usize {
        &mut self.virt as *mut usize
    }

    pub fn system() -> Self {
        Self::new(Box::new(SystemAllocator))
    }

    /// Bytes currently allocated through this `Mem`.
    #[inline]
    pub fn total_bytes(&self) -> usize {
        #[cfg(feature = "jit_trace")]
        {
            self.total + self.virt
        }
        #[cfg(not(feature = "jit_trace"))]
        {
            self.total
        }
    }

    /// Fail (with `ErrMem`) any allocation that would push the total above `limit` bytes.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Would an allocation of `extra` more bytes stay within the limit (the check `realloc` / `alloc_split` make)?
    #[inline]
    pub fn would_fit(&self, extra: usize) -> bool {
        match self.total.checked_add(self.ext).and_then(|t| t.checked_add(extra)) {
            Some(p) => p <= self.lim(),
            None => false,
        }
    }

    /// `luaM_realloc_`-like primitive. `old == 0`/null `ptr` means "new block"; `new == 0` frees and
    /// returns `Ok(None)`.
    ///
    /// # Safety
    /// `ptr` must be null or a live block obtained from this `Mem` with size exactly `old`.
    pub unsafe fn realloc(
        &mut self,
        ptr: *mut u8,
        old: usize,
        new: usize,
    ) -> LuaResult<Option<NonNull<u8>>> {
        debug_assert!(!ptr.is_null() || old == 0);
        debug_assert!(self.total >= old);
        let projected = (self.total - old).checked_add(new).ok_or(Status::ErrMem)?;
        if new > old && projected.saturating_add(self.ext) > self.lim() {
            return Err(Status::ErrMem);
        }
        // SAFETY: forwarded caller contract.
        let p = unsafe { self.alloc.realloc(ptr, old, new) };
        if new == 0 {
            self.total -= old;
            return Ok(None);
        }
        match NonNull::new(p) {
            Some(nn) => {
                self.total = projected;
                Ok(Some(nn))
            }
            None => Err(Status::ErrMem),
        }
    }

    /// Charge `size` bytes of memory held outside this allocator (JIT code) to the limit, without
    /// touching the collector-visible total. False (nothing charged) when the limit would be exceeded.
    pub fn charge_ext(&mut self, size: usize) -> bool {
        match (self.total + self.ext).checked_add(size) {
            Some(p) if p <= self.lim() => {
                self.ext += size;
                true
            }
            _ => false,
        }
    }

    /// Bytes currently charged with [`Mem::charge_ext`] (JIT code pages and the compiler's Rust-heap
    /// structures).
    pub fn ext_bytes(&self) -> usize {
        self.ext
    }

    /// Refund `size` bytes previously charged with [`Mem::charge_ext`] (JIT code freed with its prototype).
    pub fn uncharge_ext(&mut self, size: usize) {
        debug_assert!(self.ext >= size, "uncharge_ext below zero");
        self.ext = self.ext.saturating_sub(size);
    }

    /// Allocate `size` bytes of which only `counted` count as collector-visible memory.
    pub fn alloc_split(&mut self, size: usize, counted: usize) -> LuaResult<NonNull<u8>> {
        debug_assert!(counted <= size && size > 0);
        let projected = (self.total + self.ext)
            .checked_add(size)
            .ok_or(Status::ErrMem)?;
        if projected > self.lim() {
            return Err(Status::ErrMem);
        }
        // SAFETY: null pointer / old == 0 is the "new block" case.
        let p = unsafe { self.alloc.realloc(std::ptr::null_mut(), 0, size) };
        let nn = NonNull::new(p).ok_or(Status::ErrMem)?;
        self.total += counted;
        self.ext += size - counted;
        Ok(nn)
    }

    /// Free a block from [`Mem::alloc_split`].
    ///
    /// # Safety
    /// `ptr` must be a live block from `alloc_split` with the same `size` and `counted`.
    pub unsafe fn free_split(&mut self, ptr: *mut u8, size: usize, counted: usize) {
        // SAFETY: forwarded caller contract.
        unsafe { self.alloc.realloc(ptr, size, 0) };
        self.total -= counted;
        self.ext -= size - counted;
    }

    /// Allocate a `size`-byte block of which only `counted <= size` bytes are accounted: the collector-visible total and the
    /// memory-limit check see exactly `counted` (the rest is spare capacity the owner can grow into with [`Mem::recount`], C22-E).
    /// With `counted == size` this is [`Mem::alloc`].
    #[inline]
    pub fn alloc_counted(&mut self, size: usize, counted: usize) -> LuaResult<NonNull<u8>> {
        debug_assert!(counted <= size && size > 0);
        let projected = self.total.checked_add(counted).ok_or(Status::ErrMem)?;
        if projected.saturating_add(self.ext) > self.lim() {
            return Err(Status::ErrMem);
        }
        // SAFETY: the "new block" case.
        let p = unsafe { self.alloc.alloc_block(size) };
        match NonNull::new(p) {
            Some(nn) => {
                self.total = projected;
                Ok(nn)
            }
            None => Err(Status::ErrMem),
        }
    }

    /// Free a block from [`Mem::alloc_counted`].
    ///
    /// # Safety
    /// `ptr` must be a live block from `alloc_counted` with the same `size`; `counted` is the number of bytes accounted for it
    /// now (after any [`Mem::recount`]).
    #[inline]
    pub unsafe fn free_counted(&mut self, ptr: *mut u8, size: usize, counted: usize) {
        debug_assert!(self.total >= counted && counted <= size);
        // SAFETY: forwarded caller contract; freeing never fails.
        unsafe { self.alloc.free_block(ptr, size) };
        self.total -= counted;
    }

    /// Change the accounted size of a block that is not reallocated (it has spare capacity) from `old` to `new` bytes, with the
    /// limit check `realloc` makes when growing.
    #[inline]
    pub fn recount(&mut self, old: usize, new: usize) -> LuaResult<()> {
        debug_assert!(self.total >= old);
        if new > old {
            let projected = self.total.checked_add(new - old).ok_or(Status::ErrMem)?;
            if projected.saturating_add(self.ext) > self.lim() {
                return Err(Status::ErrMem);
            }
            self.total = projected;
        } else {
            self.total -= old - new;
        }
        Ok(())
    }

    /// Allocate `size` (> 0) bytes.
    pub fn alloc(&mut self, size: usize) -> LuaResult<NonNull<u8>> {
        debug_assert!(size > 0);
        // (the checks of `realloc` for `old == 0`)
        let projected = self.total.checked_add(size).ok_or(Status::ErrMem)?;
        if projected.saturating_add(self.ext) > self.lim() {
            return Err(Status::ErrMem);
        }
        // SAFETY: the "new block" case.
        let p = unsafe { self.alloc.alloc_block(size) };
        match NonNull::new(p) {
            Some(nn) => {
                self.total = projected;
                Ok(nn)
            }
            None => Err(Status::ErrMem),
        }
    }

    /// Free a block.
    ///
    /// # Safety
    /// `ptr` must be a live block from this `Mem` of exactly `size` bytes; it is dead afterwards.
    pub unsafe fn free(&mut self, ptr: *mut u8, size: usize) {
        if size != 0 {
            debug_assert!(self.total >= size);
            // SAFETY: forwarded caller contract; freeing never fails.
            unsafe { self.alloc.free_block(ptr, size) };
            self.total -= size;
        }
    }
}

/// Fallible buffer of `Copy` elements (no automatic drop: the owner calls [`RawBuf::free`] with the
/// same `Mem`). Capacity changes are the only operation; contents are the owner's business.
pub struct RawBuf<T: Copy> {
    ptr: Option<NonNull<T>>,
    cap: usize,
}

impl<T: Copy> RawBuf<T> {
    pub const fn new() -> Self {
        RawBuf { ptr: None, cap: 0 }
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Pointer to element 0 (dangling-free: null when `cap == 0`).
    #[inline]
    pub fn as_ptr(&self) -> *mut T {
        match self.ptr {
            Some(p) => p.as_ptr(),
            None => std::ptr::null_mut(),
        }
    }

    /// Resize to exactly `new_cap` elements (grow or shrink), preserving the common prefix.
    /// New elements are uninitialised. On failure the buffer is unchanged.
    pub fn try_resize(&mut self, mem: &mut Mem, new_cap: usize) -> LuaResult<()> {
        const { assert!(std::mem::align_of::<T>() <= BLOCK_ALIGN) };
        if new_cap == self.cap {
            return Ok(());
        }
        let esz = std::mem::size_of::<T>();
        let old_bytes = self.cap * esz; // cannot overflow: it was allocated
        let new_bytes = new_cap.checked_mul(esz).ok_or(Status::ErrMem)?;
        // SAFETY: (ptr, old_bytes) is the live block owned by this buffer (or null/0).
        let r = unsafe { mem.realloc(self.as_ptr().cast(), old_bytes, new_bytes) }?;
        self.ptr = r.map(NonNull::cast);
        self.cap = new_cap;
        Ok(())
    }

    /// Release the storage.
    pub fn free(&mut self, mem: &mut Mem) {
        // Shrinking to zero never fails.
        let _ = self.try_resize(mem, 0);
    }
}

impl<T: Copy> Default for RawBuf<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_free_accounting() {
        let mut m = Mem::system();
        let p = m.alloc(100).unwrap();
        assert_eq!(m.total_bytes(), 100);
        assert_eq!(p.as_ptr() as usize % BLOCK_ALIGN, 0);
        // SAFETY: p is a live 100-byte block from m.
        let q = unsafe { m.realloc(p.as_ptr(), 100, 300) }.unwrap().unwrap();
        assert_eq!(m.total_bytes(), 300);
        // SAFETY: q is a live 300-byte block from m.
        unsafe { m.free(q.as_ptr(), 300) };
        assert_eq!(m.total_bytes(), 0);
    }

    #[test]
    fn limit_gives_errmem_and_keeps_block() {
        let mut m = Mem::system();
        m.set_limit(1000);
        let p = m.alloc(600).unwrap();
        assert_eq!(m.alloc(600).unwrap_err(), Status::ErrMem);
        // SAFETY: p is a live 600-byte block.
        let r = unsafe { m.realloc(p.as_ptr(), 600, 2000) };
        assert_eq!(r.unwrap_err(), Status::ErrMem);
        assert_eq!(m.total_bytes(), 600);
        // SAFETY: p still live.
        unsafe { m.free(p.as_ptr(), 600) };
    }

    #[test]
    fn huge_request_fails_cleanly() {
        let mut m = Mem::system();
        assert_eq!(m.alloc(usize::MAX / 2).unwrap_err(), Status::ErrMem);
        assert_eq!(m.alloc(usize::MAX).unwrap_err(), Status::ErrMem);
        assert_eq!(m.total_bytes(), 0);
    }

    #[test]
    fn pool_classes_realloc_and_reuse() {
        let mut a = PoolAllocator::new();
        // SAFETY: every block is used with the size it was allocated with.
        unsafe {
            let p = a.realloc(std::ptr::null_mut(), 0, 40);
            assert_eq!(p as usize % BLOCK_ALIGN, 0);
            p.write_bytes(7, 40);
            // same class: same block
            assert_eq!(a.realloc(p, 40, 48), p);
            // small -> small (other class), small -> large, large -> small keep the prefix
            let q = a.realloc(p, 48, 200);
            assert_eq!(*q.add(39), 7);
            let r = a.realloc(q, 200, 4000);
            assert_eq!(*r.add(39), 7);
            let s = a.realloc(r, 4000, 30);
            assert_eq!(*s.add(29), 7);
            a.realloc(s, 30, 0);
            // a freed block is reused by its class
            let t = a.realloc(std::ptr::null_mut(), 0, 32);
            assert_eq!(t, s);
            a.realloc(t, 32, 0);
            // many blocks: several arenas
            let v: Vec<*mut u8> = (0..20000)
                .map(|i| a.realloc(std::ptr::null_mut(), 0, 16 + i % 400))
                .collect();
            for (i, &b) in v.iter().enumerate() {
                b.write_bytes(1, 16 + i % 400);
            }
            for (i, &b) in v.iter().enumerate() {
                a.realloc(b, 16 + i % 400, 0);
            }
        }
    }

    #[test]
    fn rawbuf_grow_shrink() {
        let mut m = Mem::system();
        let mut b: RawBuf<u64> = RawBuf::new();
        b.try_resize(&mut m, 10).unwrap();
        for i in 0..10 {
            // SAFETY: i < cap, storage live.
            unsafe { b.as_ptr().add(i).write(i as u64 * 3) };
        }
        b.try_resize(&mut m, 1000).unwrap();
        assert_eq!(m.total_bytes(), 8000);
        // SAFETY: first 10 elements were initialised and preserved.
        assert_eq!(unsafe { b.as_ptr().add(9).read() }, 27);
        assert!(b.try_resize(&mut m, usize::MAX / 4).is_err());
        assert_eq!(b.cap(), 1000);
        b.try_resize(&mut m, 4).unwrap();
        assert_eq!(m.total_bytes(), 32);
        b.free(&mut m);
        assert_eq!(m.total_bytes(), 0);
        assert!(b.as_ptr().is_null());
    }
}
