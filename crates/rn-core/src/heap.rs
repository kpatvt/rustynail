//! The `Heap`: allocator + accounting + string table + the `allgc` object list.
//!
//! No collection yet (task B2 plugs mark/sweep into the object list); everything is freed on `Drop`.
//! `unsafe` here is raw-pointer plumbing for object creation/linking/freeing.
//!
//! **Invariants:** (H1) every object reachable from `allgc` was allocated by `self.mem` with exactly
//! `string::alloc_size(kind, len)` bytes (for strings) and is freed only through `free_string`;
//! (H2) `strt.hash` holds `strt.size` bucket heads (null or live short strings, chained via `hnext`);
//! `strt.size` is a power of two; (H3) every short string is in exactly one bucket and also in `allgc`.
use crate::alloc::{Allocator, Mem, RawBuf};
use crate::gc::{GcHeader, GcState, WHITE0BIT, WHITEBITS};
use crate::host::CompilerHost;
use crate::meta::{TM_N, TM_NAMES};
use crate::status::{LuaResult, Status};
use crate::string::{
    alloc_size, counted_size, mut_hash, short_key_hash, short_key_hash_fresh, LAZY_HASH, MUT_CHAIN_CMP_HASH_ONLY, ExtFixedString, ExtFree, ExtString, LuaString, StrKind,
    StrRef, StrU, MAX_SHORT_LEN,
};
use crate::value::{Value, LUA_VLNGSTR, LUA_VSHRSTR};
use std::ffi::c_void;
use std::ptr::NonNull;

/// `MINSTRTABSIZE`.
const MIN_STRTAB_SIZE: usize = 128;
/// `MAXSTRTB` (`luaM_limitN(INT_MAX, TString*)`).
const MAX_STRTAB_SIZE: usize = (i32::MAX as usize) / std::mem::size_of::<*mut LuaString>();

struct StringTable {
    hash: RawBuf<*mut LuaString>,
    size: usize,
    nuse: usize,
}

pub struct Heap {
    pub(crate) mem: Mem,
    pub(crate) seed: u32,
    strt: StringTable,
    /// Head of the list of all collectable objects (linked through `GcHeader::next`).
    pub(crate) allgc: *mut GcHeader,
    /// Current white bit new objects get.
    pub(crate) current_white: u8,
    /// Collector state (see `gc/mod.rs`).
    pub(crate) gc: GcState,
    /// Interned, fixed names of the metamethod events (`G(L)->tmname`).
    pub(crate) tm_names: [*mut LuaString; TM_N],
    /// F2: the interned, fixed string `"__metatable"` (the field `getmetatable` / `setmetatable` look up; traces embed its address).
    pub(crate) metatable_name: *mut LuaString,
    /// F1: the carried traversal index of `next` (`TableRef::next_hinted`); only a hint, validated by every use.
    pub next_hint: u32,
    /// B3-6: the 256 interned, fixed one-byte strings (never collected, found without hashing).
    pub(crate) onebyte: [*mut LuaString; 256],
}

impl Heap {
    /// New heap on the system allocator.
    pub fn new(seed: u32) -> LuaResult<Heap> {
        Self::with_mem(Mem::system(), seed)
    }

    /// New heap on a custom allocator (`lua_newstate(f, ud)` equivalent).
    pub fn with_allocator(a: Box<dyn Allocator>, seed: u32) -> LuaResult<Heap> {
        Self::with_mem(Mem::new(a), seed)
    }

    fn with_mem(mem: Mem, seed: u32) -> LuaResult<Heap> {
        let mut h = Heap {
            mem,
            seed,
            strt: StringTable {
                hash: RawBuf::new(),
                size: 0,
                nuse: 0,
            },
            allgc: std::ptr::null_mut(),
            current_white: WHITE0BIT,
            gc: GcState::new(),
            tm_names: [std::ptr::null_mut(); TM_N],
            metatable_name: std::ptr::null_mut(),
            next_hint: 0,
            onebyte: [std::ptr::null_mut(); 256],
        };
        h.strt.hash.try_resize(&mut h.mem, MIN_STRTAB_SIZE)?; // on error `h` drops cleanly
                                                              // SAFETY: freshly allocated buffer of MIN_STRTAB_SIZE slots.
        unsafe { clear_slots(h.strt.hash.as_ptr(), 0, MIN_STRTAB_SIZE) };
        h.strt.size = MIN_STRTAB_SIZE;
        for (i, name) in TM_NAMES.iter().enumerate() {
            let s = h.new_str(name.as_bytes())?;
            h.fix_object(s.as_ptr().cast());
            h.tm_names[i] = s.as_ptr();
        }
        let s = h.new_str(b"__metatable")?;
        h.fix_object(s.as_ptr().cast());
        h.metatable_name = s.as_ptr();
        for b in 0..=255u8 {
            let s = h.new_str(&[b])?;
            h.fix_object(s.as_ptr().cast());
            h.onebyte[b as usize] = s.as_ptr();
        }
        Ok(h)
    }

    /// `luaC_newobj`: allocate `size` bytes for an object of variant `tt`, initialise its header with
    /// the current white and link it into `allgc`.
    ///
    /// The caller must initialise **every other field** before the next heap operation (the object is
    /// already on `allgc`, so a half-initialised object must never be observable by the collector).
    pub(crate) fn alloc_obj(&mut self, size: usize, tt: u8) -> LuaResult<*mut GcHeader> {
        let raw = self.mem.alloc(size)?;
        let p = raw.as_ptr().cast::<GcHeader>();
        // SAFETY: fresh block of `size >= size_of::<GcHeader>()` bytes, 16-aligned.
        unsafe {
            p.write(GcHeader::new(self.allgc, tt, self.current_white));
        }
        self.allgc = p;
        Ok(p)
    }

    /// The interned, fixed name string of a metamethod event.
    pub fn tm_name(&self, e: crate::meta::Tm) -> StrRef {
        // SAFETY: `tm_names` are live fixed short strings created in `with_mem`.
        StrRef(unsafe { NonNull::new_unchecked(self.tm_names[e as usize]) })
    }

    /// F2: the interned, fixed string `"__metatable"`.
    pub fn metatable_name(&self) -> StrRef {
        // SAFETY: created and fixed in `with_mem`.
        StrRef(unsafe { NonNull::new_unchecked(self.metatable_name) })
    }

    /// Total bytes currently allocated (GC accounting).
    pub fn total_bytes(&self) -> usize {
        self.mem.total_bytes()
    }

    /// Access to the allocation layer (limits, accounting).
    pub fn mem_mut(&mut self) -> &mut Mem {
        &mut self.mem
    }

    /// Number of interned short strings.
    pub fn strtab_len(&self) -> usize {
        self.strt.nuse
    }

    /// Current string-table bucket count.
    pub fn strtab_size(&self) -> usize {
        self.strt.size
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    // ---- string table (lstring.c) ---------------------------------------------------------------

    /// `luaS_resize`: on allocation failure the table keeps its old size.
    pub fn resize_strtab(&mut self, nsize: usize) {
        let osize = self.strt.size;
        if nsize == osize || nsize == 0 || !nsize.is_power_of_two() {
            return;
        }
        let vect = self.strt.hash.as_ptr();
        if nsize < osize {
            // SAFETY: H2; rehash moves the chains of the soon-to-be-dropped part into [0, nsize).
            unsafe { rehash(vect, osize, nsize, self.seed) };
        }
        match self.strt.hash.try_resize(&mut self.mem, nsize) {
            Err(_) => {
                if nsize < osize {
                    // SAFETY: buffer unchanged (still osize slots); redistribute back.
                    unsafe { rehash(self.strt.hash.as_ptr(), nsize, osize, self.seed) };
                }
            }
            Ok(()) => {
                let vect = self.strt.hash.as_ptr();
                if nsize > osize {
                    // SAFETY: buffer now has nsize slots; first osize are valid, clear the rest.
                    unsafe {
                        clear_slots(vect, osize, nsize);
                        rehash(vect, osize, nsize, self.seed);
                    }
                }
                self.strt.size = nsize;
            }
        }
    }

    fn grow_strtab(&mut self) {
        if self.strt.size <= MAX_STRTAB_SIZE / 2 {
            self.resize_strtab(self.strt.size * 2);
        }
    }

    /// `luaS_remove`: unlink a short string from the table (for the sweeper).
    ///
    /// # Safety
    /// `ts` must be a live short string currently in this heap's table.
    pub unsafe fn remove_short(&mut self, ts: *mut LuaString) {
        // SAFETY: H2/H3: bucket chain contains `ts`.
        unsafe {
            let idx = LuaString::strtab_index(ts, self.seed, self.strt.size);
            let head = self.strt.hash.as_ptr().add(idx);
            if *head == ts {
                *head = LuaString::hnext(ts);
            } else {
                let mut q = *head;
                while LuaString::hnext(q) != ts {
                    q = LuaString::hnext(q);
                }
                LuaString::set_hnext(q, LuaString::hnext(ts));
            }
        }
        self.strt.nuse -= 1;
    }

    /// F3b (JIT class-A helpers): the bytes that creating the string `s` (`luaS_newlstr`) would add to the allocation total,
    /// without creating it: `Some(0)` for a short string that is interned already, the object size for a new one, and
    /// `None` when creating it would also grow the string table (the caller treats that as "not now"). A read.
    pub fn jit_str_bytes(&self, s: &[u8]) -> Option<usize> {
        if s.len() > MAX_SHORT_LEN {
            return alloc_size(StrKind::LongReg, s.len());
        }
        let h = short_key_hash(s, self.seed);
        let idx = (h as usize) & (self.strt.size - 1);
        // SAFETY: H2 -- bucket heads and `hnext` links are null or live short strings (a read, as in `intern_short`).
        unsafe {
            let mut p = *self.strt.hash.as_ptr().add(idx);
            while !p.is_null() {
                if ((*p).hdr.hash.get() == h || ((*p).hdr.extra.get() != 0 && !mut_hash(MUT_CHAIN_CMP_HASH_ONLY))) && (*p).len() == s.len() && LuaString::bytes_raw(p) == s {
                    return Some(0);
                }
                p = LuaString::hnext(p);
            }
        }
        if self.strt.nuse >= self.strt.size {
            return None;
        }
        alloc_size(StrKind::Short, s.len())
    }

    /// P-B (fast concat): [`Heap::jit_str_bytes`] and the lookup of the later [`Heap::jit_str_commit`] in one pass, with a single
    /// hash of `s` (at most [`MAX_SHORT_LEN`] bytes). Returns the hash, the interned string if there is one (null otherwise) and the bytes
    /// creating it would add (`Some(0)` when it exists, `None` when the string table would also grow). A read.
    #[inline(always)]
    pub fn jit_str_probe(&self, s: &[u8]) -> (u32, *mut LuaString, Option<usize>) {
        self.jit_str_probe_impl::<false>(s)
    }

    /// [`Heap::jit_str_probe`] for bytes written by byte stores right before (the hash is read byte by byte: [`crate::string::fast_hash_fresh`]; the same result).
    #[inline(always)]
    pub fn jit_str_probe_fresh(&self, s: &[u8]) -> (u32, *mut LuaString, Option<usize>) {
        self.jit_str_probe_impl::<true>(s)
    }

    #[inline(always)]
    fn jit_str_probe_impl<const FRESH: bool>(&self, s: &[u8]) -> (u32, *mut LuaString, Option<usize>) {
        debug_assert!(s.len() <= MAX_SHORT_LEN);
        if s.len() == 1 {
            // B3-6: fixed, always present (the commit of a found string only returns it)
            let p = self.onebyte[s[0] as usize];
            if !p.is_null() {
                return (0, p, Some(0));
            }
        }
        let h = if FRESH { short_key_hash_fresh(s, self.seed) } else { short_key_hash(s, self.seed) };
        let idx = (h as usize) & (self.strt.size - 1);
        // SAFETY: H2 -- bucket heads and `hnext` links are null or live short strings (a read, as in `intern_short`).
        unsafe {
            let mut p = *self.strt.hash.as_ptr().add(idx);
            while !p.is_null() {
                if ((*p).hdr.hash.get() == h || ((*p).hdr.extra.get() != 0 && !mut_hash(MUT_CHAIN_CMP_HASH_ONLY))) && (*p).len() == s.len() && LuaString::bytes_raw(p) == s {
                    return (h, p, Some(0));
                }
                p = LuaString::hnext(p);
            }
        }
        if self.strt.nuse >= self.strt.size {
            return (h, std::ptr::null_mut(), None);
        }
        (h, std::ptr::null_mut(), alloc_size(StrKind::Short, s.len()))
    }

    /// P-B: the second half of [`Heap::jit_str_probe`]: the string `s` (the probe's `hash` and `found`; nothing was allocated or
    /// swept since) -- the interned one, resurrected if dead but unswept (`intern_short`), or a new one linked into the table. `None` if
    /// the allocation fails (nothing is changed).
    ///
    /// # Safety
    /// `(hash, found)` come from `jit_str_probe(s)` on this heap with no allocation, collection or string-table change in between,
    /// and the probe said that no table growth is needed.
    pub unsafe fn jit_str_commit(&mut self, s: &[u8], hash: u32, found: *mut LuaString) -> Option<StrRef> {
        // SAFETY: per the contract (H2: `found` is a live string of the chain; the bucket index is within the table).
        unsafe {
            if !found.is_null() {
                if (*found).hdr.marked & (self.current_white ^ WHITEBITS) != 0 {
                    (*found).hdr.marked ^= WHITEBITS;
                }
                return Some(StrRef(NonNull::new_unchecked(found)));
            }
            let idx = (hash as usize) & (self.strt.size - 1);
            let ts = self.create_inline(StrKind::Short, s, hash).ok()?;
            let head = self.strt.hash.as_ptr().add(idx);
            LuaString::init_hnext(ts.as_ptr(), *head);
            *head = ts.as_ptr();
            if !LAZY_HASH {
                LuaString::promote_hash(ts.as_ptr()); // (after the link: promotion writes the tag into the link word)
            }
            self.strt.nuse += 1;
            Some(StrRef(ts))
        }
    }

    /// [`Heap::jit_str_bytes`] for a long string of `len` bytes (always a new object).
    pub fn jit_str_bytes_long(&self, len: usize) -> Option<usize> {
        alloc_size(StrKind::LongReg, len)
    }

    fn intern_short(&mut self, s: &[u8]) -> LuaResult<NonNull<LuaString>> {
        self.intern_short_impl::<false>(s)
    }

    #[inline(always)]
    fn intern_short_impl<const FRESH: bool>(&mut self, s: &[u8]) -> LuaResult<NonNull<LuaString>> {
        let h = if FRESH { short_key_hash_fresh(s, self.seed) } else { short_key_hash(s, self.seed) };
        let idx = (h as usize) & (self.strt.size - 1);
        // SAFETY: H2 — bucket heads and `hnext` links are null or live short strings.
        unsafe {
            let mut p = *self.strt.hash.as_ptr().add(idx);
            while !p.is_null() {
                if ((*p).hdr.hash.get() == h || ((*p).hdr.extra.get() != 0 && !mut_hash(MUT_CHAIN_CMP_HASH_ONLY))) && (*p).len() == s.len() && LuaString::bytes_raw(p) == s
                {
                    // resurrect if dead-but-unswept (`isdead` -> `changewhite`)
                    if (*p).hdr.marked & (self.current_white ^ WHITEBITS) != 0 {
                        (*p).hdr.marked ^= WHITEBITS;
                    }
                    return Ok(NonNull::new_unchecked(p));
                }
                p = LuaString::hnext(p);
            }
        }
        if self.strt.nuse >= self.strt.size {
            self.grow_strtab();
        }
        let idx = (h as usize) & (self.strt.size - 1);
        let ts = self.create_inline(StrKind::Short, s, h)?;
        // SAFETY: `ts` is a fresh live string; bucket idx is within the table.
        unsafe {
            let head = self.strt.hash.as_ptr().add(idx);
            LuaString::init_hnext(ts.as_ptr(), *head);
            *head = ts.as_ptr();
        }
        if !LAZY_HASH {
            // SAFETY: `ts` is a fresh live short string (promoted after the link: promotion writes the tag into the link word).
            unsafe { LuaString::promote_hash(ts.as_ptr()) };
        }
        self.strt.nuse += 1;
        Ok(ts)
    }

    /// Short string made of the bytes of the `n` string values at `parts` in order (`tl` bytes in
    /// total, `tl <= MAX_SHORT_LEN`), interned without an intermediate buffer: the hash and the
    /// comparisons with the bucket chain run over the parts, and a new string is filled from them
    /// directly (`luaV_concat` + `internshrstr`).
    ///
    /// # Safety
    /// `parts` must point to `n` values that are live strings (anchored by the caller) with `tl`
    /// bytes in total; nothing may move them during the call.
    pub unsafe fn intern_concat(
        &mut self,
        parts: *const Value,
        n: usize,
        tl: usize,
    ) -> LuaResult<StrRef> {
        debug_assert!(tl <= MAX_SHORT_LEN);
        // SAFETY: per the contract; H2 for the chain.
        unsafe {
            let part = |j: usize| -> &[u8] {
                LuaString::bytes_raw(
                    Value::load(parts.add(j))
                        .as_strref()
                        .unwrap_unchecked()
                        .as_ptr(),
                )
            };
            let mut buf = [0u8; MAX_SHORT_LEN];
            let mut off = 0;
            for j in 0..n {
                let b = part(j);
                std::ptr::copy_nonoverlapping(b.as_ptr(), buf.as_mut_ptr().add(off), b.len());
                off += b.len();
            }
            Ok(StrRef(self.intern_short_impl::<true>(&buf[..tl])?))
        }
    }

    /// A regular (collector-counted, `luaS_createlngstrobj`) long string of `len` bytes written by
    /// `fill` in place. `fill` must not allocate.
    ///
    /// # Safety
    /// `fill` must initialise all `len` bytes at the pointer it receives.
    pub unsafe fn new_long_str_with(
        &mut self,
        len: usize,
        fill: impl FnOnce(*mut u8),
    ) -> LuaResult<StrRef> {
        debug_assert!(len > MAX_SHORT_LEN);
        let p = self.create_inline_with(StrKind::LongReg, len, self.seed, fill)?;
        Ok(StrRef(p))
    }

    /// Allocate and link a string with inline bytes (`Short` or `LongReg`) and a trailing NUL;
    /// `fill` writes the `len` content bytes.
    fn create_inline_with(
        &mut self,
        kind: StrKind,
        len: usize,
        hash: u32,
        fill: impl FnOnce(*mut u8),
    ) -> LuaResult<NonNull<LuaString>> {
        let size = alloc_size(kind, len).ok_or(Status::ErrMem)?;
        let raw = self.mem.alloc(size)?;
        let tt = if kind == StrKind::Short {
            LUA_VSHRSTR
        } else {
            LUA_VLNGSTR
        };
        let p = raw.as_ptr().cast::<LuaString>();
        // SAFETY: as in `create_inline`; `fill` writes exactly the `len` content bytes.
        unsafe {
            let contents = raw.as_ptr().add(std::mem::size_of::<LuaString>());
            fill(contents);
            *contents.add(len) = 0;
            let hdr = if kind == StrKind::Short {
                LuaString::short_header(self.allgc, tt, self.current_white, len, hash)
            } else {
                LuaString::long_header(self.allgc, tt, self.current_white, kind, hash)
            };
            p.write(LuaString {
                hdr,
                u: if kind == StrKind::Short {
                    std::cell::UnsafeCell::new(StrU {
                        hnext: std::ptr::null_mut(),
                    })
                } else {
                    std::cell::UnsafeCell::new(StrU { len })
                },
            });
            self.allgc = p.cast();
            Ok(NonNull::new_unchecked(p))
        }
    }

    /// Allocate and link a string with inline bytes (`Short` or `LongReg`) and a trailing NUL.
    fn create_inline(
        &mut self,
        kind: StrKind,
        s: &[u8],
        hash: u32,
    ) -> LuaResult<NonNull<LuaString>> {
        // SAFETY: `dst` has room for `s.len()` bytes.
        self.create_inline_with(kind, s.len(), hash, |dst| unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len())
        })
    }

    /// A string of `len` bytes written by `fill` (taken out of the `Option` only once the string
    /// object exists, so an allocation failure can be retried). Long strings are built in place;
    /// short ones go through a small buffer and the string table.
    pub fn new_str_filled<F>(&mut self, len: usize, fill: &mut Option<F>) -> LuaResult<StrRef>
    where
        F: FnOnce(&mut [std::mem::MaybeUninit<u8>]),
    {
        if len <= MAX_SHORT_LEN {
            let mut tmp = [std::mem::MaybeUninit::<u8>::uninit(); MAX_SHORT_LEN];
            if let Some(f) = fill.take() {
                f(&mut tmp[..len]);
            }
            // SAFETY: `fill` initialised the first `len` bytes.
            let bytes = unsafe { std::slice::from_raw_parts(tmp.as_ptr().cast::<u8>(), len) };
            return self.new_str(bytes);
        }
        // big results of library buffers are not collector-visible memory (see `StrKind::LongBuf`)
        // (5.4's `luaL_pushresult` copies the buffer into a regular, counted string)
        let kind = if len > BUFFER_STR_MIN && !cfg!(rustynail_lua54) {
            StrKind::LongBuf
        } else {
            StrKind::LongReg
        };
        let size = alloc_size(kind, len).ok_or(Status::ErrMem)?;
        let raw = if kind == StrKind::LongBuf {
            self.mem
                .alloc_split(size, counted_size(kind, len).ok_or(Status::ErrMem)?)?
        } else {
            self.mem.alloc(size)?
        };
        let p = raw.as_ptr().cast::<LuaString>();
        // SAFETY: fresh block of `size_of::<LuaString>() + len + 1` bytes, 16-aligned; the struct is
        // fully initialised (and the NUL written) before `fill` runs on the `len` content bytes; no
        // collection can happen meanwhile (the callback does not allocate).
        unsafe {
            let contents = raw.as_ptr().add(std::mem::size_of::<LuaString>());
            *contents.add(len) = 0;
            p.write(LuaString {
                hdr: LuaString::long_header(
                    self.allgc,
                    LUA_VLNGSTR,
                    self.current_white,
                    kind,
                    self.seed,
                ),
                u: std::cell::UnsafeCell::new(StrU { len }),
            });
            self.allgc = p.cast();
            if let Some(f) = fill.take() {
                f(std::slice::from_raw_parts_mut(
                    contents.cast::<std::mem::MaybeUninit<u8>>(),
                    len,
                ));
            }
            Ok(StrRef(NonNull::new_unchecked(p)))
        }
    }

    /// A string taking over the bytes of `v` (no copy for long ones: the block becomes an external
    /// string freed with the object, like a `luaL_Buffer` box in the reference). Long results are
    /// outside the collector's accounting (and the memory limit), as in the reference.
    pub fn new_str_from_vec(&mut self, mut v: Vec<u8>) -> LuaResult<StrRef> {
        if v.len() <= BUFFER_STR_MIN || cfg!(rustynail_lua54) {
            // (5.4's `luaL_pushresult` copies the buffer into a regular, counted string)
            return self.new_str(&v);
        }
        let len = v.len();
        v.push(0);
        let b: Box<[u8]> = v.into_boxed_slice(); // capacity == len + 1 exactly
        let p = Box::into_raw(b).cast::<u8>();
        // SAFETY: `p` points to `len + 1` bytes (NUL-terminated) that stay valid until `free_boxed`
        // runs at the string's death (or at once if creation fails), `free_boxed` matching the
        // allocation made by `into_boxed_slice`.
        unsafe { self.new_external_str(p, len, Some(free_boxed), std::ptr::null_mut()) }
    }

    /// [`Heap::new_str`] for bytes written by byte stores right before (a number formatted into a buffer, a concatenation assembled in a stack buffer): the hash
    /// of a short string is read byte by byte ([`crate::string::fast_hash_fresh`], BUNDLE-P P3); the same string, the same hash.
    pub fn new_str_fresh(&mut self, s: &[u8]) -> LuaResult<StrRef> {
        if s.len() == 1 {
            let p = self.onebyte[s[0] as usize];
            if !p.is_null() {
                // SAFETY: a fixed live string.
                return Ok(StrRef(unsafe { NonNull::new_unchecked(p) }));
            }
        }
        let p = if s.len() <= MAX_SHORT_LEN {
            self.intern_short_impl::<true>(s)?
        } else {
            self.create_inline(StrKind::LongReg, s, self.seed)?
        };
        Ok(StrRef(p))
    }

    /// `luaS_newlstr`: interns short strings, creates long ones fresh.
    pub fn new_str(&mut self, s: &[u8]) -> LuaResult<StrRef> {
        if s.len() == 1 {
            let p = self.onebyte[s[0] as usize];
            if !p.is_null() {
                // SAFETY: a fixed live string.
                return Ok(StrRef(unsafe { NonNull::new_unchecked(p) }));
            }
        }
        let p = if s.len() <= MAX_SHORT_LEN {
            self.intern_short(s)?
        } else {
            // long strings carry the seed until their hash is first requested
            self.create_inline(StrKind::LongReg, s, self.seed)?
        };
        Ok(StrRef(p))
    }

    /// `luaS_newextlstr`: a long string over caller-owned bytes (`lua_pushexternalstring`).
    /// `s[len]` must be 0. With `falloc == None` the bytes are never freed; otherwise
    /// `falloc(ud, s, len + 1, 0)` runs when the string dies (or immediately if object creation fails).
    ///
    /// # Safety
    /// `s` must point to `len + 1` readable bytes that stay valid and unchanged until the string is
    /// freed (or forever if `falloc` is `None`); `falloc`/`ud` must be a valid pairing.
    pub unsafe fn new_external_str(
        &mut self,
        s: *const u8,
        len: usize,
        falloc: Option<ExtFree>,
        ud: *mut c_void,
    ) -> LuaResult<StrRef> {
        let kind = if falloc.is_some() {
            StrKind::ExtMem
        } else {
            StrKind::ExtFixed
        };
        let size = alloc_size(kind, len).expect("fixed size");
        let raw = match self.mem.alloc(size) {
            Ok(r) => r,
            Err(e) => {
                if let Some(f) = falloc {
                    // SAFETY: caller contract — free the external block on failure like the reference.
                    unsafe { f(ud, s as *mut c_void, len + 1, 0) };
                }
                return Err(e);
            }
        };
        let base = LuaString {
            hdr: LuaString::long_header(
                self.allgc,
                LUA_VLNGSTR,
                self.current_white,
                kind,
                self.seed,
            ),
            u: std::cell::UnsafeCell::new(StrU { len }),
        };
        let p = raw.as_ptr().cast::<LuaString>();
        // SAFETY: `raw` is a fresh block of `size` bytes (>= the struct written), 16-aligned.
        unsafe {
            if kind == StrKind::ExtMem {
                raw.as_ptr().cast::<ExtString>().write(ExtString {
                    base,
                    contents: s,
                    falloc,
                    ud,
                });
            } else {
                raw.as_ptr()
                    .cast::<ExtFixedString>()
                    .write(ExtFixedString { base, contents: s });
            }
            self.allgc = p.cast();
            Ok(StrRef(NonNull::new_unchecked(p)))
        }
    }

    /// `luaS_normstr`: intern an external string if it is short, else keep it.
    pub fn normalize_str(&mut self, s: StrRef) -> LuaResult<StrRef> {
        let ts = s.get();
        if ts.len() > MAX_SHORT_LEN || ts.is_short() {
            Ok(s)
        } else {
            // copy the bytes before interning (the table may allocate)
            let bytes = ts.as_bytes().to_vec();
            self.new_str(&bytes)
        }
    }

    /// Bytes of a string (borrow tied to the heap).
    #[inline]
    pub fn str_bytes(&self, s: StrRef) -> &[u8] {
        s.get().as_bytes()
    }

    /// Head of bucket `b` of the string table (verification).
    pub(crate) fn strtab_bucket(&self, b: usize) -> *mut LuaString {
        assert!(b < self.strt.size);
        // SAFETY: H2 — `b` is within the bucket array.
        unsafe { *self.strt.hash.as_ptr().add(b) }
    }

    /// Iterate the object list (for the sweeper and tests).
    pub fn all_objects(&self) -> AllObjects<'_> {
        AllObjects {
            cur: self.allgc,
            _h: std::marker::PhantomData,
        }
    }

    /// Frees a string object.
    ///
    /// # Safety
    /// `p` must be a live string of this heap, already unlinked from `allgc` (and from the string table
    /// if short); it is dead afterwards.
    pub(crate) unsafe fn free_string(&mut self, p: *mut LuaString) {
        // SAFETY: H1 — size matches the allocation size; ExtMem callbacks follow the caller contract
        // given at creation time.
        unsafe {
            let kind = (*p).kind();
            let len = (*p).len();
            if kind == StrKind::ExtMem {
                let e = p.cast::<ExtString>();
                if let Some(f) = (*e).falloc {
                    f((*e).ud, (*e).contents as *mut c_void, len + 1, 0);
                }
            }
            let size = alloc_size(kind, len).expect("valid size");
            if kind == StrKind::LongBuf {
                let counted = counted_size(kind, len).expect("valid size");
                self.mem.free_split(p.cast(), size, counted);
            } else {
                self.mem.free(p.cast(), size);
            }
        }
    }
}

/// Minimum length of a library-buffer result that is kept out of the collector's accounting
/// (`LUAL_BUFFERSIZE` of the reference).
pub const BUFFER_STR_MIN: usize = 1024;

/// `ExtFree` for blocks from `Box<[u8]>`: `osize` is the exact allocation size.
unsafe extern "C" fn free_boxed(
    _ud: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    _nsize: usize,
) -> *mut c_void {
    // SAFETY: `ptr`/`osize` describe a live `Box<[u8]>` allocation (see `new_str_from_vec`).
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            ptr.cast::<u8>(),
            osize,
        )));
    }
    std::ptr::null_mut()
}

/// Iterator over all objects (headers).
pub struct AllObjects<'a> {
    cur: *mut GcHeader,
    _h: std::marker::PhantomData<&'a Heap>,
}

impl Iterator for AllObjects<'_> {
    type Item = *mut GcHeader;
    fn next(&mut self) -> Option<*mut GcHeader> {
        if self.cur.is_null() {
            None
        } else {
            let c = self.cur;
            // SAFETY: H1 — list nodes are live headers.
            self.cur = unsafe { (*c).next };
            Some(c)
        }
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        self.free_all_objects();
        self.strt.hash.free(&mut self.mem);
        #[cfg(feature = "jit_trace")]
        {
            // J5 sinking: the virtual bytes of sunk (never allocated) tables are not a leak
            let v = self.mem.virtual_bytes();
            self.mem.uncharge_virtual(v);
        }
        debug_assert_eq!(self.mem.total_bytes(), 0, "heap leaked memory");
    }
}

/// `tablerehash`'s clearing step.
///
/// # Safety
/// `v` has at least `to` slots.
unsafe fn clear_slots(v: *mut *mut LuaString, from: usize, to: usize) {
    for i in from..to {
        // SAFETY: i < to <= capacity.
        unsafe { v.add(i).write(std::ptr::null_mut()) };
    }
}

/// `tablerehash`: redistribute chains of slots `[0, osize)` into `[0, nsize)` (power-of-two sizes).
///
/// # Safety
/// `v` has `max(osize, nsize)` slots; slots `[nsize, osize)` (when shrinking) or `[osize, nsize)`
/// (when growing, already cleared) are valid; chains consist of live short strings.
unsafe fn rehash(v: *mut *mut LuaString, osize: usize, nsize: usize, seed: u32) {
    // SAFETY: per the function contract.
    unsafe {
        if nsize < osize {
            // move the chains from the dropped part into the kept part (every string of slot `i` has `hash & (osize - 1) == i`, so its new slot is
            // `i & (nsize - 1)`: no hash is read, which for a promoted string would mean recomputing `fast_hash` from its bytes)
            let _ = seed;
            for i in nsize..osize {
                let mut p = *v.add(i);
                *v.add(i) = std::ptr::null_mut();
                while !p.is_null() {
                    let next = LuaString::hnext(p);
                    let h = i & (nsize - 1);
                    LuaString::set_hnext(p, *v.add(h));
                    *v.add(h) = p;
                    p = next;
                }
            }
        } else {
            // growing (or restoring a shrink): every chain in [0, osize) may contain strings that
            // belong to the new slots
            for i in 0..osize {
                let mut p = *v.add(i);
                *v.add(i) = std::ptr::null_mut();
                while !p.is_null() {
                    let next = LuaString::hnext(p);
                    let h = LuaString::strtab_index(p, seed, nsize);
                    LuaString::set_hnext(p, *v.add(h));
                    *v.add(h) = p;
                    p = next;
                }
            }
        }
    }
}

impl CompilerHost for Heap {
    type Str = StrRef;

    fn intern(&mut self, bytes: &[u8]) -> LuaResult<StrRef> {
        self.new_str(bytes)
    }

    fn bytes(&self, s: StrRef) -> &[u8] {
        self.str_bytes(s)
    }

    fn str_id(s: StrRef) -> usize {
        s.as_ptr().addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn heap() -> Heap {
        Heap::new(0x5eed).unwrap()
    }

    #[test]
    fn short_strings_are_interned() {
        let mut h = heap();
        let base_len = h.strtab_len();
        let a = h.new_str(b"hello").unwrap();
        let b = h.new_str(b"hello").unwrap();
        let c = h.new_str(b"hellp").unwrap();
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_ne!(a.as_ptr(), c.as_ptr());
        assert!(a.get().is_short());
        assert_eq!(h.str_bytes(a), b"hello");
        assert_eq!(h.strtab_len(), base_len + 2);
        let e1 = h.new_str(b"").unwrap();
        let e2 = h.new_str(b"").unwrap();
        assert_eq!(e1.as_ptr(), e2.as_ptr());
        let forty = vec![b'x'; 40];
        let f1 = h.new_str(&forty).unwrap();
        let f2 = h.new_str(&forty).unwrap();
        assert!(f1.get().is_short());
        assert_eq!(f1.as_ptr(), f2.as_ptr());
    }

    #[test]
    fn long_strings_are_distinct_but_equal() {
        let mut h = heap();
        let bytes = vec![b'y'; 41];
        let a = h.new_str(&bytes).unwrap();
        let b = h.new_str(&bytes).unwrap();
        assert!(!a.get().is_short());
        assert_ne!(a.as_ptr(), b.as_ptr());
        assert_eq!(a, b); // content equality
        assert_eq!(a.get().hash(), b.get().hash());
        let c = h.new_str(&[b'z'; 41]).unwrap();
        assert_ne!(a, c);
        assert!(Value::string(a).raw_equal(&Value::string(b)));
        assert!(!Value::string(a).raw_equal(&Value::string(c)));
        assert_eq!(a.get().as_bytes()[40], b'y');
    }

    #[test]
    fn embedded_nul_and_binary() {
        let mut h = heap();
        let s = h.new_str(b"a\0b\xff").unwrap();
        assert_eq!(h.str_bytes(s), b"a\0b\xff");
        let t = h.new_str(b"a\0b\xff").unwrap();
        assert_eq!(s.as_ptr(), t.as_ptr());
    }

    #[test]
    fn table_grows_and_keeps_everything() {
        let mut h = heap();
        let base_len = h.strtab_len();
        let n = if cfg!(miri) { 300 } else { 5000 };
        let strs: Vec<_> = (0..n)
            .map(|i| h.new_str(format!("str{i}").as_bytes()).unwrap())
            .collect();
        assert!(h.strtab_size() >= n);
        for (i, s) in strs.iter().enumerate() {
            let again = h.new_str(format!("str{i}").as_bytes()).unwrap();
            assert_eq!(again.as_ptr(), s.as_ptr());
        }
        assert_eq!(h.strtab_len(), base_len + n);
        // shrink and re-grow keeps lookups intact
        h.resize_strtab(128);
        assert_eq!(h.strtab_size(), 128);
        for (i, s) in strs.iter().enumerate() {
            assert_eq!(
                h.new_str(format!("str{i}").as_bytes()).unwrap().as_ptr(),
                s.as_ptr()
            );
        }
        assert_eq!(h.all_objects().count(), n); // fixed names live on `fixedgc`
    }

    #[test]
    fn memory_accounting_and_errmem() {
        let mut h = heap();
        let before = h.total_bytes();
        let _ = h.new_str(b"abc").unwrap();
        assert_eq!(
            h.total_bytes() - before,
            std::mem::size_of::<LuaString>() + 4
        );
        let lim = h.total_bytes() + 100;
        h.mem_mut().set_limit(lim);
        assert_eq!(h.new_str(&[b'q'; 200]).unwrap_err(), Status::ErrMem);
        // heap still usable
        let _ = h.new_str(b"ok").unwrap();
    }

    #[test]
    fn external_strings() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static FREED: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn freer(
            _ud: *mut c_void,
            p: *mut c_void,
            osize: usize,
            nsize: usize,
        ) -> *mut c_void {
            assert_eq!(nsize, 0);
            FREED.fetch_add(osize, Ordering::SeqCst);
            // SAFETY: test passes a Box<[u8]> leaked with exactly `osize` bytes.
            drop(unsafe {
                Box::from_raw(std::ptr::slice_from_raw_parts_mut(p.cast::<u8>(), osize))
            });
            std::ptr::null_mut()
        }
        let mut h = heap();
        let mut data = vec![b'e'; 100];
        data.push(0);
        let leaked = Box::leak(data.into_boxed_slice());
        // SAFETY: `leaked` has len+1 bytes ending in NUL and is freed only through `freer`.
        let s = unsafe {
            h.new_external_str(leaked.as_mut_ptr(), 100, Some(freer), std::ptr::null_mut())
        }
        .unwrap();
        assert_eq!(h.str_bytes(s).len(), 100);
        assert!(!s.get().is_short());
        static FIXED: [u8; 6] = *b"fixed\0";
        // SAFETY: static NUL-terminated bytes, never freed.
        let f =
            unsafe { h.new_external_str(FIXED.as_ptr(), 5, None, std::ptr::null_mut()) }.unwrap();
        assert_eq!(h.str_bytes(f), b"fixed");
        let n = h.normalize_str(f).unwrap();
        assert!(n.get().is_short());
        assert_eq!(h.new_str(b"fixed").unwrap().as_ptr(), n.as_ptr());
        drop(h);
        assert_eq!(FREED.load(Ordering::SeqCst), 101);
    }

    #[test]
    fn compiler_host_impl() {
        let mut h = heap();
        let a = CompilerHost::intern(&mut h, b"x").unwrap();
        assert_eq!(CompilerHost::bytes(&h, a), b"x");
    }

    #[test]
    fn value_roundtrip_string() {
        let mut h = heap();
        let s = h.new_str(b"abc").unwrap();
        let v = Value::string(s);
        assert!(v.is_string() && v.is_short_string() && v.is_collectable());
        assert_eq!(v.as_string().unwrap().as_bytes(), b"abc");
        assert_eq!(v.type_of(), crate::value::LuaType::String);
        assert_eq!(std::mem::size_of::<Value>(), 16);
    }
}
