//! Strings (port of `TString` / `lstring.c`).
//!
//! * short strings (`len <= LUAI_MAXSHORTLEN` = 40) are interned in the heap's string table, so equal
//!   bytes <=> same object;
//! * long strings are distinct objects with a lazily computed hash;
//! * "external" long strings (5.5 `lua_pushexternalstring`) point at caller-owned bytes, optionally with a
//!   deallocation callback.
//!
//! Object creation lives in `heap.rs` (it needs the allocator and string table); this file owns the
//! layout, the hash function and the read-only accessors. `unsafe` is confined to the pointer reads in
//! `LuaString::as_bytes` / [`StrRef`].
//!
//! **Invariants:** (S1) `contents` points to `len` readable bytes (followed by a NUL for strings made by
//! `Heap::new_str`) for as long as the object is alive; (S2) for `Short`/`LongReg` kinds `contents`
//! points into the same allocation just past the struct; (S3) a [`StrRef`] is only created by the `Heap`
//! that owns the object and is valid until that heap frees/collects the object.
use crate::gc::GcHeader;
use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::NonNull;

/// `LUAI_MAXSHORTLEN`.
pub const MAX_SHORT_LEN: usize = 40;

/// How the bytes of a string are stored (`shrlen` sign convention in the reference).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrKind {
    /// Interned short string; bytes inline.
    Short = 0,
    /// Regular long string; bytes inline.
    LongReg = 1,
    /// External bytes, never freed by Lua (`LSTRFIX`).
    ExtFixed = 2,
    /// External bytes freed through [`ExtFree`] (`LSTRMEM`).
    ExtMem = 3,
    /// Long string with inline bytes built by a library buffer: only the header counts as
    /// collector-visible memory (like the reference's `luaL_Buffer` strings).
    LongBuf = 4,
}

/// Deallocation callback for external strings (`lua_Alloc` shape: `f(ud, ptr, osize, 0)` frees).
pub type ExtFree = unsafe extern "C" fn(
    ud: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    nsize: usize,
) -> *mut c_void;

/// Length of a short string / kind of a long one, packed in the header (`shrlen`): `>= 0` is the
/// length of an interned short string, the negative values are the long-string kinds.
const SHR_LONGREG: i8 = -1;
const SHR_EXTFIXED: i8 = -2;
const SHR_EXTMEM: i8 = -3;
const SHR_LONGBUF: i8 = -4;

/// `u` of the reference: the length of a long string, or the bucket-chain link of a short one.
#[repr(C)]
pub(crate) union StrU {
    pub(crate) len: usize,
    pub(crate) hnext: *mut LuaString,
}

/// GC string object. `#[repr(C)]` with the header first. The header's spare bytes hold `extra`
/// ("PUC hash computed" for both kinds), `shrlen` and `hash` (long strings: the seed until `extra == 1`, then the
/// hash; short strings: `fast_hash(bytes) ^ seed` until `extra == 1`, then the `luaS_hash` value, see [`LAZY_HASH`]).
#[repr(C)]
pub struct LuaString {
    pub(crate) hdr: GcHeader,
    pub(crate) u: std::cell::UnsafeCell<StrU>,
}

/// Offset of the bytes (inline strings) or of the content pointer (external strings).
pub(crate) const CONTENTS_OFFSET: usize = std::mem::size_of::<LuaString>();

/// Header + content pointer, allocated for `ExtFixed` strings.
#[repr(C)]
pub(crate) struct ExtFixedString {
    pub(crate) base: LuaString,
    pub(crate) contents: *const u8,
}

/// Header + external-memory info, allocated instead of a bare `LuaString` for `ExtMem` strings.
#[repr(C)]
pub(crate) struct ExtString {
    pub(crate) base: LuaString,
    pub(crate) contents: *const u8,
    pub(crate) falloc: Option<ExtFree>,
    pub(crate) ud: *mut c_void,
}

/// Bytes allocated for a string object of the given kind/length (including the trailing NUL for
/// inline strings).
pub(crate) fn alloc_size(kind: StrKind, len: usize) -> Option<usize> {
    match kind {
        StrKind::Short | StrKind::LongReg | StrKind::LongBuf => std::mem::size_of::<LuaString>()
            .checked_add(len)?
            .checked_add(1),
        StrKind::ExtFixed => Some(std::mem::size_of::<ExtFixedString>()),
        StrKind::ExtMem => Some(std::mem::size_of::<ExtString>()),
    }
}

/// Bytes of the object that the collector accounts for (the bytes of a `LongBuf` string do not count).
pub(crate) fn counted_size(kind: StrKind, len: usize) -> Option<usize> {
    match kind {
        StrKind::LongBuf => Some(std::mem::size_of::<LuaString>()),
        _ => alloc_size(kind, len),
    }
}

/// `luaS_hash`.
#[inline]
pub fn lua_hash(s: &[u8], seed: u32) -> u32 {
    let mut h = seed ^ (s.len() as u32);
    for &b in s.iter().rev() {
        h ^= (h << 5).wrapping_add(h >> 2).wrapping_add(b as u32);
    }
    h
}

/// Lazy PUC hash for short strings ("hash-D", docs/PERF.md BUNDLE-J). A new short string stores
/// `fast_hash(bytes) ^ seed` in `hash` with `extra == 0`; the `luaS_hash` value (needed for table
/// node placement, hence for `pairs` order) is computed on first request (`promote_hash`: `extra = 1`).
/// Invariant I-H: a string with `extra == 0` is never a table node key (lookups may read the raw
/// field; every insert goes through `mainposition`, which promotes). With the cargo feature
/// `eager_hash` every short string is promoted at creation (A/B and identity tests).
pub const LAZY_HASH: bool = !cfg!(feature = "eager_hash");

/// Test-only mutants of the lazy hash (feature `jit_trace_mut`, set by `rn_jit::tr::mutants::apply_vm_mutants`): `promote_skip`,
/// `strtab_idx_raw`, `chain_cmp_hash_only`, `record_mainpos_raw`, `promote_seed0`.
#[cfg(feature = "jit_trace_mut")]
static MUT_HASH: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
pub(crate) const MUT_PROMOTE_SKIP: u16 = 1;
pub(crate) const MUT_STRTAB_IDX_RAW: u16 = 2;
pub(crate) const MUT_CHAIN_CMP_HASH_ONLY: u16 = 4;
pub(crate) const MUT_RECORD_MAINPOS_RAW: u16 = 8;
pub(crate) const MUT_PROMOTE_SEED0: u16 = 16;
pub(crate) const MUT_STRTAG_ZERO: u16 = 32;
pub(crate) const MUT_STRTAG_SHIFT: u16 = 64;
pub(crate) const MUT_STRTAG_WIDE: u16 = 128;
pub(crate) const MUT_BYTETAIL_SKIP: u16 = 256;

#[cfg(feature = "jit_trace_mut")]
fn set_mut_hash_bit(bit: u16, on: bool) {
    if on {
        MUT_HASH.fetch_or(bit, std::sync::atomic::Ordering::Relaxed);
    } else {
        MUT_HASH.fetch_and(!bit, std::sync::atomic::Ordering::Relaxed);
    }
}
/// Select the lazy-hash mutant `promote_skip` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_promote_skip(on: bool) {
    set_mut_hash_bit(MUT_PROMOTE_SKIP, on);
}

/// Select the lazy-hash mutant `strtab_idx_raw` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_strtab_idx_raw(on: bool) {
    set_mut_hash_bit(MUT_STRTAB_IDX_RAW, on);
}

/// Select the lazy-hash mutant `chain_cmp_hash_only` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_chain_cmp_hash_only(on: bool) {
    set_mut_hash_bit(MUT_CHAIN_CMP_HASH_ONLY, on);
}

/// Select the lazy-hash mutant `record_mainpos_raw` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_record_mainpos_raw(on: bool) {
    set_mut_hash_bit(MUT_RECORD_MAINPOS_RAW, on);
}

/// Select the lazy-hash mutant `promote_seed0` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_promote_seed0(on: bool) {
    set_mut_hash_bit(MUT_PROMOTE_SEED0, on);
}

/// Select the strtab-tag mutant `strtag_zero` (feature `jit_trace_mut`): the tag of a promoted string reads as 0.
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_strtag_zero(on: bool) {
    set_mut_hash_bit(MUT_STRTAG_ZERO, on);
}

/// Select the strtab-tag mutant `strtag_shift` (feature `jit_trace_mut`): the bucket index is taken from the tag shifted by one.
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_strtag_shift(on: bool) {
    set_mut_hash_bit(MUT_STRTAG_SHIFT, on);
}

/// Select the byte-tail mutant `bytetail_hi_byte_skip` (feature `jit_trace_mut`): the byte-load tail of `fast_hash` (4..7 bytes) drops the highest byte of its first word.
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_bytetail_skip(on: bool) {
    set_mut_hash_bit(MUT_BYTETAIL_SKIP, on);
}

/// Select the strtab-tag mutant `strtag_wide` (feature `jit_trace_mut`): the tag is used for string tables with more than `1 << TAG_BITS` buckets.
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_strtag_wide(on: bool) {
    set_mut_hash_bit(MUT_STRTAG_WIDE, on);
}

#[inline(always)]
pub(crate) fn mut_hash(bit: u16) -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT_HASH.load(std::sync::atomic::Ordering::Relaxed) & bit != 0;
    #[cfg(not(feature = "jit_trace_mut"))]
    {
        let _ = bit;
        false
    }
}

/// The string-table (interning) hash of a short string.
#[inline(always)]
pub fn short_key_hash(s: &[u8], seed: u32) -> u32 {
    fast_hash(s) ^ seed
}

/// [`short_key_hash`] for bytes that were assembled by byte stores just before (BUNDLE-P P3, see [`fast_hash_fresh`]); the same value.
#[inline(always)]
pub fn short_key_hash_fresh(s: &[u8], seed: u32) -> u32 {
    fast_hash_fresh(s) ^ seed
}

/// BUNDLE-P P3: `false` = [`fast_hash_fresh`] is [`fast_hash`] (the 4-byte loads of J2 everywhere).
const BYTE_TAIL: bool = true;

/// The little-endian `u32` at `p` read as four volatile byte loads (as a `u64`).
///
/// # Safety
/// `p..p+4` must be readable.
#[inline(always)]
unsafe fn read_le_bytes4(p: *const u8) -> u64 {
    // SAFETY: per the contract.
    unsafe {
        u64::from(core::ptr::read_volatile(p))
            | u64::from(core::ptr::read_volatile(p.add(1))) << 8
            | u64::from(core::ptr::read_volatile(p.add(2))) << 16
            | u64::from(core::ptr::read_volatile(p.add(3))) << 24
    }
}

/// Unseeded word-at-a-time hash of a short string (len <= 40); the string table keys on
/// `fast_hash(s) ^ seed`, so the seed can be recovered from the header while the PUC hash is not computed.
#[inline(always)]
pub fn fast_hash(s: &[u8]) -> u32 {
    fast_hash_impl::<false>(s)
}

/// [`fast_hash`] (the same value) for a short string whose bytes were written by byte stores right before (`concat_num`'s stack buffer, number formatting, the
/// trace allocation helpers that format a number): its 4..7 byte tail is loaded byte by byte so that the loads are forwarded from the stores (two overlapping
/// 4-byte loads wait for the stores to retire: `lib/str_idx_concat_key` +4-5 % cycles since J2, `lib/tostring_int` +10 %). Not for strings that were stored
/// long ago (`lib/m_type`: +5.8 % instructions, +7.5 % cycles when every call hashes a static `"integer"` this way).
#[inline(always)]
pub fn fast_hash_fresh(s: &[u8]) -> u32 {
    fast_hash_impl::<BYTE_TAIL>(s)
}

#[inline(always)]
fn fast_hash_impl<const FRESH: bool>(s: &[u8]) -> u32 {
    #[inline(always)]
    fn mix(a: u64, b: u64) -> u64 {
        let r = (a as u128) * (b as u128);
        (r as u64) ^ ((r >> 64) as u64)
    }
    let n = s.len();
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut h = (n as u64).wrapping_mul(K);
    // the last byte is added after the mixing: strings that differ only in it land in neighbouring
    // buckets (string-table locality for sequential keys, like `luaS_hash`)
    let last: u64;
    // SAFETY: all reads are within `s` (overlapping unaligned loads).
    unsafe {
        let p = s.as_ptr();
        if n >= 8 {
            let mut i = 0;
            while i + 8 < n {
                let w = (p.add(i) as *const u64).read_unaligned();
                h = mix(h ^ w, K);
                i += 8;
            }
            let w = (p.add(n - 8) as *const u64).read_unaligned();
            last = w >> 56;
            h = mix(h ^ (w & 0x00FF_FFFF_FFFF_FFFF), K);
        } else if n >= 4 {
            // BUNDLE-P P3: for a string just assembled by byte stores, two overlapping 4-byte loads cannot be store-forwarded (a load wider than the
            // stores it overlaps waits for them to retire). Byte loads (volatile: LLVM would merge them back into the 4-byte loads) are forwarded.
            // The words are the same values.
            let (a, b) = if FRESH {
                let a = read_le_bytes4(p);
                (
                    if mut_hash(MUT_BYTETAIL_SKIP) {
                        a & 0x00FF_FFFF
                    } else {
                        a
                    },
                    read_le_bytes4(p.add(n - 4)),
                )
            } else {
                (
                    (p as *const u32).read_unaligned() as u64,
                    (p.add(n - 4) as *const u32).read_unaligned() as u64,
                )
            };
            last = b >> 24;
            h = mix(h ^ (a | ((b & 0xFF_FFFF) << 32)), K);
        } else if n > 0 {
            let a = *p as u64;
            let b = *p.add(n / 2) as u64;
            last = *p.add(n - 1) as u64;
            h = mix(h ^ (a | (b << 8)), K);
        } else {
            last = 0;
        }
    }
    ((h ^ (h >> 32)) as u32).wrapping_add(last as u32)
}

/// Strtab tag ("hash-D" follow-up, docs/PERF.md BUNDLE-M): when a short string is promoted (`promote_hash`: its `hash` field changes from the
/// string-table hash `fast_hash(bytes) ^ seed` to the PUC hash) the low [`TAG_BITS`] bits of the string-table hash are kept in the `hnext` word next to
/// the chain link, so that growing the table and unlinking a dead string need no `fast_hash` recompute from the bytes of a promoted string.
/// A string that is not promoted has tag bits 0 and is indexed by its `hash` field. Layout of the word on x86-64: bits 48..63 hold
/// `tag >> 3`, bits 0..2 hold `tag & 7` (strings are 8-aligned), bits 3..47 are the pointer (user addresses are below 2^47). Elsewhere
/// ([`TAGGED`] false) the word is the plain pointer and the index of a promoted string is recomputed. A table with more than `1 << TAG_BITS`
/// buckets also recomputes (the tag does not hold enough bits). Invariant I-T: the tag of a promoted string in the table is `strtab_hash & TAG_MASK`.
const TAGGED: bool = cfg!(all(target_arch = "x86_64", target_pointer_width = "64"));
/// Number of hash bits kept in the `hnext` word.
pub(crate) const TAG_BITS: u32 = if TAGGED { 19 } else { 0 };
const TAG_MASK: usize = (1usize << TAG_BITS) - 1;
const PTR_MASK: usize = if TAGGED {
    0x0000_FFFF_FFFF_FFF8
} else {
    usize::MAX
};

impl LuaString {
    /// Chain link of a short string (the tag bits cleared).
    ///
    /// # Safety
    /// `p` must point to a live short string.
    #[inline(always)]
    pub(crate) unsafe fn hnext(p: *mut LuaString) -> *mut LuaString {
        // SAFETY: per the contract.
        unsafe { (*Self::hnext_slot(p)).map_addr(|a| a & PTR_MASK) }
    }

    /// Sets the chain link of a short string, keeping its tag.
    ///
    /// # Safety
    /// `p` must point to a live short string; `n` null or a live short string.
    #[inline(always)]
    pub(crate) unsafe fn set_hnext(p: *mut LuaString, n: *mut LuaString) {
        // SAFETY: per the contract.
        unsafe {
            let slot = Self::hnext_slot(p);
            debug_assert!(
                n.addr() & !PTR_MASK == 0,
                "string address does not fit the tagged link"
            );
            *slot = n.map_addr(|a| a | ((*slot).addr() & !PTR_MASK));
        }
    }

    /// Sets the chain link of a short string that enters the table (not promoted yet: tag bits 0).
    ///
    /// # Safety
    /// As [`LuaString::set_hnext`]; the string is not promoted.
    #[inline(always)]
    pub(crate) unsafe fn init_hnext(p: *mut LuaString, n: *mut LuaString) {
        // SAFETY: per the contract.
        unsafe {
            debug_assert!(
                n.addr() & !PTR_MASK == 0,
                "string address does not fit the tagged link"
            );
            *Self::hnext_slot(p) = n;
        }
    }

    /// The tag of a short string in the table (`strtab_hash & TAG_MASK`).
    ///
    /// # Safety
    /// `p` must point to a live short string that was linked with [`LuaString::init_hnext`].
    #[inline(always)]
    pub(crate) unsafe fn strtab_tag(p: *mut LuaString) -> usize {
        // SAFETY: per the contract.
        let w = unsafe { (*Self::hnext_slot(p)).addr() };
        if TAGGED {
            let t = ((w >> 48) << 3) | (w & 7);
            if mut_hash(MUT_STRTAG_ZERO) {
                0
            } else {
                t
            }
        } else {
            0
        }
    }

    /// Bucket of a short string of the string table (size `size`, a power of two): from the hash field of an unpromoted string, from the tag of a
    /// promoted one while the table is small enough, else from `fast_hash` of its bytes.
    ///
    /// # Safety
    /// `p` a live short string in the table.
    #[inline(always)]
    pub(crate) unsafe fn strtab_index(p: *mut LuaString, seed: u32, size: usize) -> usize {
        // SAFETY: per the contract.
        unsafe {
            if (*p).hdr.extra.get() == 0 || mut_hash(MUT_STRTAB_IDX_RAW) {
                (*p).hdr.hash.get() as usize & (size - 1)
            } else if TAGGED && (size <= 1usize << TAG_BITS || mut_hash(MUT_STRTAG_WIDE)) {
                let t = Self::strtab_tag(p);
                (if mut_hash(MUT_STRTAG_SHIFT) {
                    t >> 1
                } else {
                    t
                }) & (size - 1)
            } else {
                (fast_hash(Self::bytes_raw(p)) ^ seed) as usize & (size - 1)
            }
        }
    }

    /// The string-table hash of a short string (fast hash ^ seed), whether or not promoted.
    ///
    /// # Safety
    /// `p` a live short string.
    #[inline(always)]
    pub(crate) unsafe fn strtab_hash(p: *const LuaString, seed: u32) -> u32 {
        unsafe {
            if (*p).hdr.extra.get() == 0 || mut_hash(MUT_STRTAB_IDX_RAW) {
                (*p).hdr.hash.get()
            } else {
                fast_hash(Self::bytes_raw(p)) ^ seed
            }
        }
    }
    /// Compute and cache the PUC hash (short: seed = field ^ fast; long: seed = field).
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn promote_hash(p: *const LuaString) {
        // SAFETY: `p` is a live string with the provenance of its whole allocation (the contents of an inline string follow the header, outside
        // what a `&LuaString` reaches: the bytes are read through `p`, not through `this`, under Stacked Borrows).
        let this = unsafe { &*p };
        let h = &this.hdr;
        if mut_hash(MUT_PROMOTE_SKIP) {
            return;
        }
        if TAGGED && this.is_short() {
            // keep the low bits of the string-table hash (the `hash` field, until the line below) in the link word: bits 48..63 hold `tag >> 3`, bits 0..2 `tag & 7`
            let t = h.hash.get() as usize & TAG_MASK;
            let slot: *mut *mut LuaString = this.u.get().cast();
            // SAFETY: a short string is linked into the table; the link word is read and written through the interior-mutable `u`.
            unsafe { *slot = (*slot).map_addr(|a| (a & PTR_MASK) | ((t >> 3) << 48) | (t & 7)) };
        }
        // SAFETY: `p` is live (above).
        let bytes = unsafe { Self::bytes_raw(p) };
        let seed = if mut_hash(MUT_PROMOTE_SEED0) {
            0
        } else if this.is_short() {
            h.hash.get() ^ fast_hash(bytes)
        } else {
            h.hash.get()
        };
        h.hash.set(lua_hash(bytes, seed));
        h.extra.set(1);
    }
    /// Header for a new short string of `len` bytes.
    pub(crate) fn short_header(
        next: *mut GcHeader,
        tt: u8,
        marked: u8,
        len: usize,
        hash: u32,
    ) -> GcHeader {
        debug_assert!(len <= MAX_SHORT_LEN);
        GcHeader {
            next,
            tt,
            marked,
            extra: Cell::new(0),
            shrlen: len as i8,
            hash: Cell::new(hash),
        }
    }

    /// Header for a new long string of the given kind (`hash` is the seed until first requested).
    pub(crate) fn long_header(
        next: *mut GcHeader,
        tt: u8,
        marked: u8,
        kind: StrKind,
        seed: u32,
    ) -> GcHeader {
        let shrlen = match kind {
            StrKind::LongReg => SHR_LONGREG,
            StrKind::ExtFixed => SHR_EXTFIXED,
            StrKind::ExtMem => SHR_EXTMEM,
            StrKind::LongBuf => SHR_LONGBUF,
            StrKind::Short => unreachable!("short strings use `short_header`"),
        };
        GcHeader {
            next,
            tt,
            marked,
            extra: Cell::new(0),
            shrlen,
            hash: Cell::new(seed),
        }
    }

    /// Address of the `hnext` link of a short string.
    ///
    /// # Safety
    /// `p` must point to a live short string.
    #[inline]
    pub(crate) unsafe fn hnext_slot(p: *mut LuaString) -> *mut *mut LuaString {
        // SAFETY: the union field is at a fixed offset in a live object.
        unsafe { (*p).u.get().cast() }
    }

    #[inline]
    pub fn kind(&self) -> StrKind {
        match self.hdr.shrlen {
            n if n >= 0 => StrKind::Short,
            SHR_LONGREG => StrKind::LongReg,
            SHR_EXTFIXED => StrKind::ExtFixed,
            SHR_LONGBUF => StrKind::LongBuf,
            _ => StrKind::ExtMem,
        }
    }
    #[inline]
    pub fn is_short(&self) -> bool {
        self.hdr.shrlen >= 0
    }
    #[inline]
    pub fn len(&self) -> usize {
        if self.hdr.shrlen >= 0 {
            self.hdr.shrlen as usize
        } else {
            // SAFETY: long strings store their length in `u.len`.
            unsafe { (*self.u.get()).len }
        }
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Pointer to the bytes of the string at `p`.
    ///
    /// # Safety
    /// `p` must point to a live string object and carry the provenance of its whole allocation
    /// (the bytes of inline strings follow the 24-byte header).
    #[inline]
    pub(crate) unsafe fn contents_ptr(p: *const LuaString) -> *const u8 {
        // SAFETY: per the contract; inline strings keep their bytes at `CONTENTS_OFFSET`, external
        // strings keep a pointer to them there.
        unsafe {
            let at = p.cast::<u8>().add(CONTENTS_OFFSET);
            if (*p).hdr.shrlen >= SHR_LONGREG || (*p).hdr.shrlen == SHR_LONGBUF {
                at
            } else {
                at.cast::<*const u8>().read()
            }
        }
    }

    /// The bytes of the string at `p`.
    ///
    /// # Safety
    /// As [`LuaString::contents_ptr`]; the result must not outlive the object.
    #[inline]
    pub(crate) unsafe fn bytes_raw<'a>(p: *const LuaString) -> &'a [u8] {
        // SAFETY: invariant S1: `len` readable bytes at the contents pointer.
        unsafe { std::slice::from_raw_parts(Self::contents_ptr(p), (*p).len()) }
    }

    /// Reserved-word marker used by the lexer (short strings).
    #[inline]
    pub fn extra(&self) -> u8 {
        self.hdr.extra.get()
    }
    pub fn set_extra(&self, e: u8) {
        self.hdr.extra.set(e)
    }
    /// The PUC hash of a short string (computed on first request).
    ///
    /// # Safety
    /// `p` a live short string with the provenance of its whole allocation (see [`LuaString::promote_hash`]).
    #[inline]
    pub unsafe fn short_hash(p: *const LuaString) -> u32 {
        // SAFETY: as documented.
        unsafe {
            debug_assert!((*p).is_short());
            if (*p).hdr.extra.get() == 0 {
                Self::promote_hash(p);
            }
            (*p).hdr.hash.get()
        }
    }
    /// `extra` (1 once the PUC hash of a short string is computed).
    #[inline]
    pub(crate) fn extra_raw(&self) -> u8 {
        self.hdr.extra.get()
    }
    /// The raw hash field (lookups only: an unpromoted string is not a key anywhere).
    #[inline]
    pub fn short_hash_raw(&self) -> u32 {
        self.hdr.hash.get()
    }
}

/// Handle to a string object owned by a `Heap`. `Copy`; equality is *content* equality (fast path:
/// same object), which is what `CompilerHost::Str` requires. See invariant S3.
#[derive(Clone, Copy)]
pub struct StrRef(pub(crate) NonNull<LuaString>);

impl StrRef {
    #[inline]
    pub fn as_ptr(self) -> *mut LuaString {
        self.0.as_ptr()
    }
    /// Borrow the object. Valid while the owning heap keeps it alive (S3).
    #[inline]
    pub fn get<'a>(self) -> StrView<'a> {
        StrView {
            p: self.0,
            _life: std::marker::PhantomData,
        }
    }
}

/// A borrowed view of a string object that keeps the provenance of the whole allocation (the
/// bytes of inline strings live right after the 24-byte header, outside what a `&LuaString` could
/// reach). Header fields are available through `Deref`.
#[derive(Clone, Copy)]
pub struct StrView<'a> {
    p: NonNull<LuaString>,
    _life: std::marker::PhantomData<&'a LuaString>,
}

impl std::ops::Deref for StrView<'_> {
    type Target = LuaString;
    #[inline]
    fn deref(&self) -> &LuaString {
        // SAFETY: S3 — the view was created by the owning heap for a live object; only the header is
        // reachable through this reference.
        unsafe { self.p.as_ref() }
    }
}

impl<'a> StrView<'a> {
    /// The bytes of the string.
    #[inline]
    pub fn as_bytes(self) -> &'a [u8] {
        // SAFETY: S1/S3 — `p` comes from the allocation of a live string object.
        unsafe { LuaString::bytes_raw(self.p.as_ptr()) }
    }
    /// `luaS_hashlongstr` / short-string hash: computes (and caches) the hash of long strings.
    pub fn hash(self) -> u32 {
        let h = &self.hdr;
        if h.extra.get() == 0 {
            // SAFETY: the view keeps the provenance of the whole allocation of a live string.
            unsafe { LuaString::promote_hash(self.p.as_ptr()) };
        }
        h.hash.get()
    }
    /// Raw pointer to the object.
    #[inline]
    pub fn as_ptr(self) -> *mut LuaString {
        self.p.as_ptr()
    }
}

impl PartialEq for StrRef {
    fn eq(&self, other: &Self) -> bool {
        if self.0 == other.0 {
            return true;
        }
        let (a, b) = (self.get(), other.get());
        // distinct short strings are never equal (interning)
        !(a.is_short() && b.is_short()) && a.as_bytes() == b.as_bytes()
    }
}
impl Eq for StrRef {}

impl std::fmt::Debug for StrRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "StrRef({:?})",
            String::from_utf8_lossy(self.get().as_bytes())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The J2 `fast_hash` before the byte-load tail of BUNDLE-P P3 (plain unaligned loads): the string-table hash must not change.
    fn fast_hash_reference(s: &[u8]) -> u32 {
        fn mix(a: u64, b: u64) -> u64 {
            let r = (a as u128) * (b as u128);
            (r as u64) ^ ((r >> 64) as u64)
        }
        const K: u64 = 0x9E37_79B9_7F4A_7C15;
        let n = s.len();
        let mut h = (n as u64).wrapping_mul(K);
        let last: u64;
        let rd64 = |i: usize| u64::from_le_bytes(s[i..i + 8].try_into().unwrap());
        let rd32 = |i: usize| u64::from(u32::from_le_bytes(s[i..i + 4].try_into().unwrap()));
        if n >= 8 {
            let mut i = 0;
            while i + 8 < n {
                h = mix(h ^ rd64(i), K);
                i += 8;
            }
            let w = rd64(n - 8);
            last = w >> 56;
            h = mix(h ^ (w & 0x00FF_FFFF_FFFF_FFFF), K);
        } else if n >= 4 {
            let (a, b) = (rd32(0), rd32(n - 4));
            last = b >> 24;
            h = mix(h ^ (a | ((b & 0xFF_FFFF) << 32)), K);
        } else if n > 0 {
            let (a, b) = (u64::from(s[0]), u64::from(s[n / 2]));
            last = u64::from(s[n - 1]);
            h = mix(h ^ (a | (b << 8)), K);
        } else {
            last = 0;
        }
        ((h ^ (h >> 32)) as u32).wrapping_add(last as u32)
    }

    /// BUNDLE-P P3: `fast_hash` (byte loads for 4..7 bytes) and `short_key_hash` (`fast_hash ^ seed`) equal the plain-load reference for every length
    /// 0..=64 on fixed, all-0x00, all-0xFF, ascending and pseudo-random contents, at every alignment offset.
    #[test]
    fn fast_hash_equals_reference_all_lengths() {
        let mut x = 0x1234_5678_9ABC_DEF0u64;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut buf = vec![0u8; 64 + 16];
        for round in 0..200 {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = match round {
                    0 => 0,
                    1 => 0xFF,
                    2 => i as u8,
                    _ => rnd() as u8,
                };
            }
            for off in 0..8 {
                for n in 0..=64usize {
                    let s = &buf[off..off + n];
                    let want = fast_hash_reference(s);
                    assert_eq!(fast_hash(s), want, "len {n} off {off} round {round}");
                    assert_eq!(
                        fast_hash_fresh(s),
                        want,
                        "fresh: len {n} off {off} round {round}"
                    );
                    for seed in [0u32, 1, 0x9E37_79B9, 0xFFFF_FFFF, rnd() as u32] {
                        assert_eq!(short_key_hash(s, seed), want ^ seed);
                        assert_eq!(short_key_hash_fresh(s, seed), want ^ seed);
                    }
                }
            }
        }
    }

    #[test]
    fn hash_matches_reference_algorithm() {
        // seed ^ len, then h ^= (h<<5)+(h>>2)+byte from the last byte backwards
        let mut h: u32 = 7 ^ 3;
        for &b in b"cba" {
            h ^= (h << 5).wrapping_add(h >> 2).wrapping_add(b as u32);
        }
        assert_eq!(lua_hash(b"abc", 7), h);
        assert_eq!(lua_hash(b"", 5), 5);
    }
}
