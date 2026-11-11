//! Lua tables: port of Lua 5.5 `ltable.c`/`ltable.h`.
//!
//! * array part: values (8-byte payloads) stored *inverted* before a `u32` length hint, followed by one
//!   tag byte per slot (`concretesize`), exactly like the reference — 9 bytes per slot;
//! * hash part: chained scatter table with Brent's variation, 24-byte [`Node`]s, `lastfree` pointer
//!   stored just before the node array for hash parts with at least 8 nodes;
//! * identical sizing (`computesizes`/`rehash`), border search (`getn` with the `lenhint`) and
//!   traversal order (`next`) as the reference, so `#t` and `pairs` order match it exactly for
//!   non-pointer keys.
//!
//! **5.4** (`--cfg rustynail_lua54`): the same layout with the 5.4 `ltable.c` policies as `cfg`
//! arms: `computesizes`/`rehash`, a `lastfree` pointer for every hash part, the array limit
//! `alimit` (kept in the hint slot, `BITRAS`) with `luaH_getn` and `luaH_getint`'s limit update
//! (`alimit_touch`), and the plain `hash_search`. `#t` and `pairs` order match the 5.4 reference.
//!
//! `unsafe` is confined to this file's raw-pointer plumbing.
//!
//! **Invariants:** (T1) `array` is null iff `asize == 0`, else it points into a block of
//! `asize * 9 + 4` bytes obtained from the heap's `Mem` at `array - asize * 8` (values at negative
//! indices, `u32` hint at 0, tags from byte 4); (T2) `node` points to `2^lsizenode` initialised nodes
//! (or to the shared immutable dummy node when `flags & BITDUMMY`); for `lsizenode >= 3` a `lastfree`
//! pointer lives in the 8 bytes before `node` within the same allocation; (T3) node chains (`next`
//! offsets) stay inside the node array; (T4) table keys that are collectable point to live objects
//! unless the key tag is `LUA_TDEADKEY` (then the pointer is only ever compared, never dereferenced).
use crate::gc::{GcHeader, BLACKBIT, WHITEBITS};
use crate::heap::Heap;
use crate::meta::{Tm, MASKFLAGS};
use crate::number::{float_to_int, F2I};
use crate::status::Status;
use crate::string::{LuaString, StrRef, LAZY_HASH};
use crate::value::*;
use std::cell::UnsafeCell;
use std::ptr::NonNull;

/// Bit in `flags` meaning "the hash part is the shared dummy node".
pub const BITDUMMY: u8 = 1 << 6;
/// Only hash parts with at least `2^LIMFORLAST` nodes carry a `lastfree` pointer (5.4: every
/// non-dummy hash part has one).
const LIMFORLAST: u8 = 3;
/// Does a hash part of `2^lsizenode` nodes carry a `lastfree` pointer?
#[inline(always)]
const fn sized_haslastfree(lsizenode: u8) -> bool {
    cfg!(rustynail_lua54) || lsizenode >= LIMFORLAST
}
/// 5.4 `BITRAS`: the array limit (kept in the `lenhint` slot) is not the real array size.
#[cfg(rustynail_lua54)]
pub const BITRAS: u8 = 1 << 7;
const MAXABITS: u32 = 31;
/// Largest array part (2^31).
const MAXASIZE: u64 = 1u64 << MAXABITS;
const MAXHBITS: u32 = MAXABITS - 1;
const MAXHSIZE: usize = 1usize << MAXHBITS;
/// Size in bytes of the `lastfree` slot.
const LIMBOX: usize = LASTFREE_BYTES;
/// Size in bytes of the `lastfree` slot, stored right before the first node (A1: the trace compiler writes it).
pub const LASTFREE_BYTES: usize = 8;

/// Results of `pset` (`HOK`...).
pub const HOK: i32 = 0;
pub const HNOTFOUND: i32 = 1;
pub const HNOTATABLE: i32 = 2;
pub const HFIRSTNODE: i32 = 3;

/// The node found (or not: null) by a short-string key lookup, handed from the interpreter's
/// inline store attempt to [`TableRef::psetshortstr_after`].
#[derive(Clone, Copy)]
pub struct StrSlot(*mut Node);

/// Errors from table operations; the VM converts them to Lua errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    /// "table index is nil"
    NilIndex,
    /// "table index is NaN"
    NanIndex,
    /// allocation failure
    Mem,
    /// "table overflow"
    Overflow,
    /// "invalid key to 'next'"
    InvalidNextKey,
}

impl From<Status> for TableError {
    fn from(_: Status) -> Self {
        TableError::Mem
    }
}

/// A1: what `insertkey` of a short-string key changes in a small hash part (see [`TableRef::plan_shortstr_insert`]): the key
/// goes to node `node` (the chain link of that node becomes `node_next`); when `node != main` the colliding node `main` (which
/// sits in its own main position) gets the chain link `main_next` and `lastfree` (if the hash part has one) moves to index
/// `lastfree`; the header flags become `flags` (`invalidateTMcache`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsertPlan {
    /// Node that receives the key.
    pub node: u8,
    /// Main position of the key.
    pub main: u8,
    /// New chain link of `main` (equal to its old one in the direct case).
    pub main_next: i32,
    /// New chain link of `node`.
    pub node_next: i32,
    /// New index of the `lastfree` pointer, when the insertion moved it.
    pub lastfree: Option<u8>,
    /// Header `flags` byte after the insertion.
    pub flags: u8,
}

/// The hash part as [`TableRef::hash_image`] reports it: per node `(key tag, key payload, chain link, value is empty)`, and the index
/// `lastfree` points at.
pub type HashImage = (Vec<(u8, u64, i32, bool)>, usize);

impl InsertPlan {
    /// The image `img` becomes when the key at address `key` is inserted as this plan says (the new value is not empty).
    pub fn apply(&self, img: &mut HashImage, key: u64) {
        img.0[self.node as usize] = (ctb(LUA_VSHRSTR), key, self.node_next, false);
        if self.node != self.main {
            img.0[self.main as usize].2 = self.main_next;
        }
        if let Some(l) = self.lastfree {
            img.1 = l as usize;
        }
    }
}

/// Hash node: value (payload + tag), key tag, chain offset, key payload. 24 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Node {
    val: Payload,
    val_tt: u8,
    key_tt: u8,
    next: i32,
    key: Payload,
}

struct SyncNode(UnsafeCell<Node>);
// SAFETY: the dummy node is never written (all writes are guarded by `isdummy` checks) and only read.
unsafe impl Sync for SyncNode {}

static DUMMY_NODE: SyncNode = SyncNode(UnsafeCell::new(Node {
    val: Payload::ZERO,
    val_tt: LUA_VEMPTY,
    key_tt: LUA_TDEADKEY,
    next: 0,
    key: Payload::ZERO,
}));

#[inline]
fn dummynode() -> *mut Node {
    DUMMY_NODE.0.get()
}

/// The GC table object.
#[repr(C)]
pub struct Table {
    // The first six fields have the layout of a `GcHeader` (16 bytes: the spare bytes after
    // `marked` hold `flags`, `lsizenode` and `asize`), so the object is 48 bytes as in the reference.
    pub(crate) next: *mut GcHeader,
    pub(crate) tt: u8,
    pub(crate) marked: u8,
    pub(crate) flags: u8,
    pub(crate) lsizenode: u8,
    pub(crate) asize: u32,
    array: *mut u8,
    node: *mut Node,
    pub(crate) metatable: *mut Table,
    pub(crate) gclist: *mut GcHeader,
}

/// Handle to a table owned by a `Heap` (valid while the heap keeps it alive).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TableRef(NonNull<Table>);

/// Test-only mutant `tbl_move_meta_skip` (N2): `arr_move` ignores the metatable guard.
#[cfg(feature = "jit_trace_mut")]
static MUT_TBL_MOVE_META_SKIP: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Select the mutant `tbl_move_meta_skip` (feature `jit_trace_mut`).
#[cfg(feature = "jit_trace_mut")]
pub fn set_mut_tbl_move_meta_skip(on: bool) {
    MUT_TBL_MOVE_META_SKIP.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[inline(always)]
fn mut_tbl_move_meta_skip() -> bool {
    #[cfg(feature = "jit_trace_mut")]
    return MUT_TBL_MOVE_META_SKIP.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(feature = "jit_trace_mut"))]
    false
}

impl TableRef {
    /// # Safety
    /// `p` must be a non-null pointer to a live table.
    #[inline]
    pub unsafe fn from_raw(p: *mut Table) -> TableRef {
        // SAFETY: caller guarantees non-null.
        TableRef(unsafe { NonNull::new_unchecked(p) })
    }
    #[inline]
    pub fn as_ptr(self) -> *mut Table {
        self.0.as_ptr()
    }
}

// ---------------------------------------------------------------------------------------------
// raw accessors
// ---------------------------------------------------------------------------------------------

#[inline]
unsafe fn sizenode(t: *const Table) -> usize {
    // SAFETY: caller passes a valid table.
    1usize << unsafe { (*t).lsizenode }
}
#[inline]
unsafe fn isdummy(t: *const Table) -> bool {
    // SAFETY: valid table.
    unsafe { (*t).flags & BITDUMMY != 0 }
}
#[inline]
unsafe fn haslastfree(t: *const Table) -> bool {
    // SAFETY: valid table.
    unsafe { sized_haslastfree((*t).lsizenode) }
}
#[inline]
unsafe fn lastfree_slot(t: *const Table) -> *mut *mut Node {
    // SAFETY: T2: with lsizenode >= 3 the slot lives right before `node`.
    unsafe { (*t).node.cast::<u8>().sub(LIMBOX).cast() }
}
#[inline]
unsafe fn gnode(t: *const Table, i: usize) -> *mut Node {
    // SAFETY: caller keeps i <= sizenode (T2).
    unsafe { (*t).node.add(i) }
}
#[inline]
unsafe fn nval(n: *const Node) -> Value {
    // SAFETY: T4/V1: stored values are valid; (tag, payload) came from `to_parts`.
    unsafe { Value::from_parts((*n).val_tt, (*n).val) }
}
#[inline]
unsafe fn nval_empty(n: *const Node) -> bool {
    // SAFETY: valid node.
    unsafe { (*n).val_tt & 0x0F == LUA_TNIL }
}
#[inline]
unsafe fn set_nval(n: *mut Node, v: &Value) {
    let (t, p) = v.to_parts();
    // SAFETY: valid node.
    unsafe {
        (*n).val_tt = t;
        (*n).val = p;
    }
}
#[inline]
unsafe fn nkey(n: *const Node) -> Value {
    // SAFETY: T4 — live (or dead, never dereferenced) key.
    unsafe { Value::from_parts((*n).key_tt, (*n).key) }
}
#[inline]
unsafe fn set_nkey(n: *mut Node, k: &Value) {
    let (t, p) = k.to_parts();
    // SAFETY: valid node.
    unsafe {
        (*n).key_tt = t;
        (*n).key = p;
    }
}
#[inline]
unsafe fn key_is_nil(n: *const Node) -> bool {
    // SAFETY: valid node.
    unsafe { (*n).key_tt == LUA_TNIL }
}
#[inline]
unsafe fn key_is_collectable(n: *const Node) -> bool {
    // SAFETY: valid node.
    unsafe { (*n).key_tt & BIT_ISCOLLECTABLE != 0 }
}

#[inline]
unsafe fn arr_tag(t: *const Table, k: usize) -> *mut u8 {
    // SAFETY: T1, k < asize.
    unsafe { (*t).array.add(4 + k) }
}
#[inline]
unsafe fn arr_val(t: *const Table, k: usize) -> *mut Payload {
    // SAFETY: T1, k < asize.
    unsafe { (*t).array.cast::<Payload>().sub(1 + k) }
}
#[inline]
unsafe fn lenhint(t: *const Table) -> *mut u32 {
    // SAFETY: T1, asize > 0.
    unsafe { (*t).array.cast() }
}
#[inline]
unsafe fn arr_get(t: *const Table, k: usize) -> Value {
    // SAFETY: T1/V1.
    unsafe { Value::from_parts(*arr_tag(t, k), *arr_val(t, k)) }
}
#[inline]
unsafe fn arr_set(t: *const Table, k: usize, v: &Value) {
    let (tag, p) = v.to_parts();
    // SAFETY: T1.
    unsafe {
        // both addresses first: the tag store could alias `(*t).array` otherwise
        let (tp, vp) = (arr_tag(t, k), arr_val(t, k));
        *tp = tag;
        *vp = p;
    }
}
#[inline]
unsafe fn arr_empty(t: *const Table, k: usize) -> bool {
    // SAFETY: T1.
    unsafe { *arr_tag(t, k) & 0x0F == LUA_TNIL }
}

/// `luaO_ceillog2`.
#[inline]
fn ceillog2(x: u32) -> u32 {
    32 - x.wrapping_sub(1).leading_zeros()
}

fn concretesize(size: u32) -> usize {
    if size == 0 {
        0
    } else {
        size as usize * 9 + 4
    }
}

/// C22-E: physical array capacity (slots) of an array part of `asize` slots: 4 for `asize` 1 and 2, `asize` otherwise (5.4:
/// always `asize`). So growing 1 -> 2 -> 4 slots (the first steps of `t[#t + 1] = x` construction: rehash only produces powers of
/// two) needs no new block, no copy and no free: the array pointer (the end of the value slots) stays, value slot `k` stays at
/// `array - 8 * (k + 1)`, the hint and the tags stay at `array + 0` / `array + 4 + k`. Only the *logical* size
/// `concretesize(asize)` is accounted (`Mem::alloc_counted` / `recount`), so `collectgarbage('count')`, GC debt, `luaH_size`
/// based accounting and the memory limit see exactly what they saw before. The block starts at `array - 8 * arrcap(asize)`; the
/// capacity follows from `asize` alone, so nothing is stored (no header flag: the trace code stores whole `flags` bytes).
/// 3 slots keep exact capacity (constructors like `table.pack(a, b, c)` do not pay for a spare slot that is never used).
#[cfg(not(rustynail_lua54))]
#[inline(always)]
fn arrcap(asize: u32) -> u32 {
    if asize.wrapping_sub(1) < 2 {
        4
    } else {
        asize
    }
}
#[cfg(rustynail_lua54)]
#[inline(always)]
fn arrcap(asize: u32) -> u32 {
    asize
}

/// Bytes that `Heap::new_table` followed by `resize(nasize, nhsize)` allocate (what the `NEWTABLE` instruction
/// counts before its `check_gc`; J5 class-A helpers decide from it whether a GC step would be due). `None` when the
/// sizes are beyond the table limits (the resize would raise an error).
pub fn new_table_bytes(nasize: u32, nhsize: u32) -> Option<usize> {
    if nasize as u64 > MAXASIZE {
        return None;
    }
    let mut n = std::mem::size_of::<Table>() + concretesize(nasize);
    if nhsize > 0 {
        let lsize = ceillog2(nhsize);
        if lsize > MAXHBITS || (1usize << lsize) > MAXHSIZE {
            return None;
        }
        n += sizehash_bytes(lsize as u8);
    }
    Some(n)
}

fn sizehash_bytes(lsizenode: u8) -> usize {
    (1usize << lsizenode) * std::mem::size_of::<Node>()
        + if sized_haslastfree(lsizenode) {
            LIMBOX
        } else {
            0
        }
}

// ---------------------------------------------------------------------------------------------
// hashing
// ---------------------------------------------------------------------------------------------

#[inline]
unsafe fn hashpow2(t: *const Table, n: u32) -> *mut Node {
    // SAFETY: index masked into range.
    unsafe { gnode(t, (n as usize) & (sizenode(t) - 1)) }
}
const fn fm_magic(l: u32) -> u64 {
    let d = ((1u64 << l) - 1) | 1;
    (u64::MAX / d).wrapping_add(1)
}
const FM: [u64; 32] = {
    let mut a = [0u64; 32];
    let mut i = 0;
    while i < 32 {
        a[i] = fm_magic(i as u32);
        i += 1;
    }
    a
};
/// `n % ((2^l - 1) | 1)` (Lemire fastmod, exact for every u32 `n`).
#[inline(always)]
fn fastmod(n: u32, l: u8) -> u32 {
    let d = ((1u64 << l) - 1) | 1;
    let low = FM[(l & 31) as usize].wrapping_mul(n as u64);
    ((low as u128 * d as u128) >> 64) as u32
}
#[inline]
unsafe fn hashmod(t: *const Table, n: u32) -> *mut Node {
    // SAFETY: remainder < sizenode.
    unsafe { gnode(t, fastmod(n, (*t).lsizenode) as usize) }
}

unsafe fn hashint(t: *const Table, i: i64) -> *mut Node {
    let ui = i as u64;
    // SAFETY: indices computed modulo (sizenode-1)|1 < sizenode.
    unsafe {
        if ui <= i32::MAX as u64 {
            gnode(t, fastmod(ui as u32, (*t).lsizenode) as usize)
        } else {
            gnode(t, (ui % (((sizenode(t) - 1) | 1) as u64)) as usize)
        }
    }
}

/// C `frexp` for finite values: returns (mantissa in [0.5,1), exponent).
fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7FF) as i32;
    if e == 0 {
        // subnormal: scale up by 2^54
        let (m, ex) = frexp(x * f64::from_bits(0x4350_0000_0000_0000));
        return (m, ex - 54);
    }
    let m = f64::from_bits((bits & !(0x7FFu64 << 52)) | (1022u64 << 52));
    (m, e - 1022)
}

/// `l_hashfloat`.
fn hashfloat(n: f64) -> u32 {
    let (m, i) = frexp(n);
    let n = m * 2147483648.0; // -(double)INT_MIN
    match crate::number::float_in_int_range(n) {
        None => 0,
        Some(ni) => {
            let u = (i as u32).wrapping_add(ni as u32);
            if u <= i32::MAX as u32 {
                u
            } else {
                !u
            }
        }
    }
}

/// `mainpositionTV`: the two common key kinds (short string, integer) inline, the rest out of line (P-TINS2:
/// the out-of-line `match` was 15-22 % of a fresh-table insert).
#[inline(always)]
unsafe fn mainposition(t: *const Table, key: &Value) -> *mut Node {
    // SAFETY: strings/objects in keys are live (T4/V1).
    unsafe {
        let tag = key.tag();
        if tag == ctb(LUA_VSHRSTR) {
            if let Some(s) = key.as_string() {
                return hashpow2(t, s.hash());
            }
        } else if tag == LUA_VNUMINT {
            return hashint(t, key.as_int().unwrap_or(0));
        }
        mainposition_slow(t, key)
    }
}

#[inline(never)]
unsafe fn mainposition_slow(t: *const Table, key: &Value) -> *mut Node {
    // SAFETY: as `mainposition`.
    unsafe {
        match key.tag() {
            LUA_VNUMINT => hashint(t, key.as_int().unwrap_or(0)),
            LUA_VNUMFLT => hashmod(t, hashfloat(key.as_float().unwrap_or(0.0))),
            x if x == ctb(LUA_VSHRSTR) => match key.as_string() {
                Some(s) => hashpow2(t, s.hash()),
                None => gnode(t, 0),
            },
            x if x == ctb(LUA_VLNGSTR) => match key.as_string() {
                Some(s) => hashpow2(t, s.hash()),
                None => gnode(t, 0),
            },
            LUA_VFALSE => hashpow2(t, 0),
            LUA_VTRUE => hashpow2(t, 1),
            _ => {
                let addr = key.to_parts().1.addr();
                hashmod(t, addr as u32)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// lookup
// ---------------------------------------------------------------------------------------------

unsafe fn equalkey(k1: &Value, n2: *const Node, deadok: bool) -> bool {
    // SAFETY: valid node; strings behind string tags are live (T4).
    unsafe {
        let kt2 = (*n2).key_tt;
        let kt1 = k1.tag();
        if kt1 != kt2 {
            if kt2 == ctb(LUA_VSHRSTR) && kt1 == ctb(LUA_VLNGSTR) {
                let a = k1.as_string().map(crate::string::StrView::as_bytes);
                let kv = nkey(n2);
                let b = kv.as_string().map(crate::string::StrView::as_bytes);
                return a.is_some() && a == b;
            }
            if deadok && kt2 == LUA_TDEADKEY && k1.is_collectable() {
                return k1.to_parts().1.addr() == (*n2).key.addr();
            }
            return false;
        }
        match kt2 {
            LUA_VNIL | LUA_VFALSE | LUA_VTRUE => true,
            LUA_VNUMINT => k1.as_int() == nkey(n2).as_int(),
            LUA_VNUMFLT => k1.as_float() == nkey(n2).as_float(),
            x if x == ctb(LUA_VLNGSTR) => {
                let a = k1.as_string().map(crate::string::StrView::as_bytes);
                let kv = nkey(n2);
                let b = kv.as_string().map(crate::string::StrView::as_bytes);
                a.is_some() && a == b
            }
            _ => k1.to_parts().1.addr() == (*n2).key.addr(),
        }
    }
}

/// `getgeneric`: node holding `key` (possibly with an empty value), or null if absent.
unsafe fn getgeneric(t: *const Table, key: &Value, deadok: bool) -> *mut Node {
    // SAFETY: chains stay inside the node array (T3).
    unsafe {
        let mut n = mainposition(t, key);
        loop {
            if equalkey(key, n, deadok) {
                return n;
            }
            let nx = (*n).next;
            if nx == 0 {
                return std::ptr::null_mut();
            }
            n = n.offset(nx as isize);
        }
    }
}

#[inline]
fn checkrange(k: i64, limit: u64) -> u32 {
    if (k as u64).wrapping_sub(1) < limit {
        k as u32
    } else {
        0
    }
}
#[inline]
fn arrayindex(k: i64) -> u32 {
    checkrange(k, MAXASIZE)
}
/// 5.4 `luaH_getint` side effect: an integer access to array index `u` (0-based, `< asize`) past
/// the array limit makes `u + 1` the new limit ("probably '#t' is here now"). A no-op in 5.5.
#[inline(always)]
unsafe fn alimit_touch(t: *const Table, u: u64) {
    #[cfg(rustynail_lua54)]
    // SAFETY: T1 — `u < asize`, so the hint slot exists.
    unsafe {
        if (*t).flags & BITRAS != 0 {
            let lim = *lenhint(t) as u64;
            if u >= lim && (u & !lim.wrapping_sub(1)) < lim {
                *lenhint(t) = (u + 1) as u32;
            }
        }
    }
    #[cfg(not(rustynail_lua54))]
    let _ = (t, u);
}
#[inline]
unsafe fn ikeyinarray(t: *const Table, k: i64) -> u32 {
    // SAFETY: valid table.
    checkrange(k, unsafe { (*t).asize } as u64)
}
unsafe fn keyinarray(t: *const Table, key: &Value) -> u32 {
    match key.as_int() {
        // SAFETY: valid table.
        Some(i) => unsafe { ikeyinarray(t, i) },
        None => 0,
    }
}

unsafe fn getintfromhash(t: *const Table, key: i64) -> *mut Node {
    // SAFETY: T3.
    unsafe {
        let mut n = hashint(t, key);
        loop {
            if (*n).key_tt == LUA_VNUMINT && (*n).key.int() == key {
                return n;
            }
            let nx = (*n).next;
            if nx == 0 {
                return std::ptr::null_mut();
            }
            n = n.offset(nx as isize);
        }
    }
}

unsafe fn hashkeyisempty(t: *const Table, key: u64) -> bool {
    // SAFETY: valid table.
    unsafe {
        let n = getintfromhash(t, key as i64);
        n.is_null() || nval_empty(n)
    }
}

#[inline(always)]
unsafe fn hgetshortstr(t: *const Table, key: *const LuaString) -> *mut Node {
    // SAFETY: T3; `key` is a live short string.
    unsafe {
        // I-H (lazy hash): a string whose PUC hash was never computed is not a key of any table, so the lookup is a miss without touching the node
        // vector (the walk would start at an arbitrary node: a cache miss the insertion that usually follows could not reuse)
        if LAZY_HASH && (*key).extra_raw() == 0 {
            return std::ptr::null_mut();
        }
        let mut n = hashpow2(t, (*key).short_hash_raw());
        loop {
            if (*n).key_tt == ctb(LUA_VSHRSTR) && (*n).key.addr() == key as usize {
                return n;
            }
            let nx = (*n).next;
            if nx == 0 {
                return std::ptr::null_mut();
            }
            n = n.offset(nx as isize);
        }
    }
}

unsafe fn hgetstr(t: *const Table, key: *const LuaString) -> *mut Node {
    // SAFETY: `key` live.
    unsafe {
        if (*key).is_short() {
            hgetshortstr(t, key)
        } else {
            getgeneric(
                t,
                &Value::string(StrRef(NonNull::new_unchecked(key as *mut LuaString))),
                false,
            )
        }
    }
}

#[inline]
unsafe fn node_value_or_nil(n: *const Node) -> Value {
    // SAFETY: n null or valid.
    unsafe {
        if n.is_null() || nval_empty(n) {
            Value::NIL
        } else {
            nval(n)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// traversal
// ---------------------------------------------------------------------------------------------

unsafe fn findindex(t: *const Table, key: &Value, asize: u32) -> Result<u32, TableError> {
    if key.is_nil() {
        return Ok(0);
    }
    // SAFETY: valid table.
    unsafe {
        let i = keyinarray(t, key);
        if i != 0 {
            return Ok(i);
        }
        let short = key.as_strref().filter(|_| key.tag() == ctb(LUA_VSHRSTR));
        let n = if let Some(s) = short {
            // `getgeneric(.., deadok)` for a short string: the same pointer as a live or dead key
            let p = s.as_ptr().addr();
            if LAZY_HASH && (*s.as_ptr()).extra_raw() == 0 {
                std::ptr::null_mut() // (I-H: not a key anywhere)
            } else {
                let mut n = hashpow2(t, (*s.as_ptr()).short_hash_raw());
                loop {
                    let kt = (*n).key_tt;
                    if (kt == ctb(LUA_VSHRSTR) || kt == LUA_TDEADKEY) && (*n).key.addr() == p {
                        break n;
                    }
                    let nx = (*n).next;
                    if nx == 0 {
                        break std::ptr::null_mut();
                    }
                    n = n.offset(nx as isize);
                }
            }
        } else {
            getgeneric(t, key, true)
        };
        if n.is_null() {
            return Err(TableError::InvalidNextKey);
        }
        let idx = n.offset_from((*t).node) as u32;
        Ok(idx + 1 + asize)
    }
}

// ---------------------------------------------------------------------------------------------
// hash part management
// ---------------------------------------------------------------------------------------------

unsafe fn setnodevector(heap: &mut Heap, t: *mut Table, size: u32) -> Result<(), TableError> {
    // SAFETY: `t` valid (possibly a temporary stack table); fresh allocation initialised below.
    unsafe {
        if size == 0 {
            (*t).node = dummynode();
            (*t).lsizenode = 0;
            (*t).flags |= BITDUMMY;
            return Ok(());
        }
        let lsize = ceillog2(size);
        if lsize > MAXHBITS || (1usize << lsize) > MAXHSIZE {
            return Err(TableError::Overflow);
        }
        let n = 1usize << lsize;
        let bytes = sizehash_bytes(lsize as u8);
        let block = heap.mem.alloc(bytes)?.as_ptr();
        let node;
        if !sized_haslastfree(lsize as u8) {
            node = block.cast::<Node>();
        } else {
            node = block.add(LIMBOX).cast::<Node>();
            *node.cast::<u8>().sub(LIMBOX).cast::<*mut Node>() = node.add(n);
        }
        for i in 0..n {
            node.add(i).write(Node {
                val: Payload::ZERO,
                val_tt: LUA_VEMPTY,
                key_tt: LUA_TNIL,
                next: 0,
                key: Payload::ZERO,
            });
        }
        (*t).node = node;
        (*t).lsizenode = lsize as u8;
        (*t).flags &= !BITDUMMY;
        Ok(())
    }
}

unsafe fn freehash(heap: &mut Heap, t: *mut Table) {
    // SAFETY: T2.
    unsafe {
        if !isdummy(t) {
            let extra = if haslastfree(t) { LIMBOX } else { 0 };
            let base = (*t).node.cast::<u8>().sub(extra);
            heap.mem.free(base, sizehash_bytes((*t).lsizenode));
        }
    }
}

unsafe fn getfreepos(t: *mut Table) -> *mut Node {
    // SAFETY: T2.
    unsafe {
        #[cfg(rustynail_lua54)]
        if isdummy(t) {
            return std::ptr::null_mut();
        }
        if haslastfree(t) {
            let slot = lastfree_slot(t);
            while *slot > (*t).node {
                *slot = (*slot).sub(1);
                if key_is_nil(*slot) {
                    return *slot;
                }
            }
        } else {
            let mut i = sizenode(t);
            while i > 0 {
                i -= 1;
                let f = gnode(t, i);
                if key_is_nil(f) {
                    return f;
                }
            }
        }
        std::ptr::null_mut()
    }
}

/// `insertkey`: insert a key known to be absent. `false` if there is no free node.
/// The free-main-position case is inline (it is what every reinsertion of `resize` and most inserts hit); a collision
/// goes to the out-of-line [`insertkey_collide`] (P-TINS5).
#[inline(always)]
unsafe fn insertkey(t: *mut Table, key: &Value, value: &Value) -> bool {
    // SAFETY: chains and indices stay inside the node array; the dummy node is never written because
    // `isdummy` forces the free-position path, which fails for it.
    unsafe {
        let mp = mainposition(t, key);
        if nval_empty(mp) && !isdummy(t) {
            set_nkey(mp, key);
            set_nval(mp, value);
            return true;
        }
        insertkey_collide(t, mp, key, value)
    }
}

/// `insertkey` past the free-main-position test: `mp` is the main position of `key` and is taken (or the table has no hash part).
#[inline(never)]
unsafe fn insertkey_collide(t: *mut Table, mut mp: *mut Node, key: &Value, value: &Value) -> bool {
    // SAFETY: as `insertkey`.
    unsafe {
        {
            let f = getfreepos(t);
            if f.is_null() {
                return false;
            }
            let mut othern = mainposition(t, &nkey(mp));
            if othern != mp {
                // colliding node is out of its main position: move it to the free slot
                while othern.offset((*othern).next as isize) != mp {
                    othern = othern.offset((*othern).next as isize);
                }
                (*othern).next = f.offset_from(othern) as i32;
                // field-wise, not `*f = *mp` (a 24-byte copy of a node stored by separate narrow stores)
                (*f).val = (*mp).val;
                (*f).val_tt = (*mp).val_tt;
                (*f).key_tt = (*mp).key_tt;
                (*f).next = (*mp).next;
                (*f).key = (*mp).key;
                if (*mp).next != 0 {
                    (*f).next += mp.offset_from(f) as i32;
                    (*mp).next = 0;
                }
                (*mp).val_tt = LUA_VEMPTY;
            } else {
                // colliding node is in its own main position: new node goes to the free slot
                if (*mp).next != 0 {
                    (*f).next = (mp.offset((*mp).next as isize)).offset_from(f) as i32;
                } else {
                    debug_assert_eq!((*f).next, 0);
                }
                (*mp).next = f.offset_from(mp) as i32;
                mp = f;
            }
        }
        set_nkey(mp, key);
        set_nval(mp, value);
        true
    }
}

/// Insert a key where there is guaranteed space, key valid, value non-nil.
unsafe fn newcheckedkey(t: *mut Table, key: &Value, value: &Value) {
    // SAFETY: valid table.
    unsafe {
        let i = keyinarray(t, key);
        if i > 0 {
            arr_set(t, i as usize - 1, value);
        } else {
            let done = insertkey(t, key, value);
            debug_assert!(done);
            let _ = done;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// rehash
// ---------------------------------------------------------------------------------------------

struct Counters {
    total: u32,
    na: u32,
    deleted: bool,
    nums: [u32; MAXABITS as usize + 1],
}

#[cfg_attr(rustynail_lua54, allow(dead_code))]
#[inline]
fn array_x_hash(na: u64, nh: u64) -> bool {
    na <= nh * 3
}

/// 5.4 `computesizes`: the largest `2^i` such that more than half of `1..2^i` is in use.
#[cfg(rustynail_lua54)]
fn computesizes(ct: &mut Counters) -> u32 {
    let mut a: u32 = 0;
    let mut na: u32 = 0;
    let mut optimal: u32 = 0;
    let mut i = 0;
    let mut twotoi: u32 = 1;
    while twotoi > 0 && ct.na > twotoi / 2 {
        a += ct.nums[i];
        if a > twotoi / 2 {
            optimal = twotoi;
            na = a;
        }
        i += 1;
        twotoi = twotoi.wrapping_mul(2);
    }
    ct.na = na;
    optimal
}

#[cfg(not(rustynail_lua54))]
fn computesizes(ct: &mut Counters) -> u32 {
    let mut a: u32 = 0;
    let mut na: u32 = 0;
    let mut optimal: u32 = 0;
    let mut i = 0;
    let mut twotoi: u32 = 1;
    while twotoi > 0 && array_x_hash(twotoi as u64, ct.na as u64) {
        let nums = ct.nums[i];
        a += nums;
        if nums > 0 && array_x_hash(twotoi as u64, a as u64) {
            optimal = twotoi;
            na = a;
        }
        i += 1;
        twotoi = twotoi.wrapping_mul(2);
    }
    ct.na = na;
    optimal
}

fn countint(key: i64, ct: &mut Counters) {
    let k = arrayindex(key);
    if k != 0 {
        ct.nums[ceillog2(k) as usize] += 1;
        ct.na += 1;
    }
}

unsafe fn numusearray(t: *const Table, ct: &mut Counters) {
    // SAFETY: T1.
    unsafe {
        let mut ause = 0u32;
        let mut i = 1u32;
        let asize = (*t).asize;
        let mut ttlg: u32 = 1;
        for lg in 0..=MAXABITS {
            let mut lc = 0u32;
            let mut lim = ttlg;
            if lim > asize {
                lim = asize;
                if i > lim {
                    break;
                }
            }
            while i <= lim {
                if !arr_empty(t, i as usize - 1) {
                    lc += 1;
                }
                i += 1;
            }
            ct.nums[lg as usize] += lc;
            ause += lc;
            ttlg = ttlg.wrapping_mul(2);
        }
        ct.total += ause;
        ct.na += ause;
    }
}

unsafe fn numusehash(t: *const Table, ct: &mut Counters) {
    // SAFETY: T2.
    unsafe {
        let mut i = sizenode(t);
        let mut total = 0u32;
        while i > 0 {
            i -= 1;
            let n = gnode(t, i);
            if nval_empty(n) {
                ct.deleted = true;
            } else {
                total += 1;
                if (*n).key_tt == LUA_VNUMINT {
                    countint((*n).key.int(), ct);
                }
            }
        }
        ct.total += total;
    }
}

/// Allocate the block of an array part of `asize > 0` slots (capacity [`arrcap`]); returns the array pointer (the end of the
/// value slots). Hint and tags are not initialised. Only the logical size is accounted.
#[inline(always)]
unsafe fn alloc_array(heap: &mut Heap, asize: u32) -> Result<*mut u8, Status> {
    // SAFETY: the pointer arithmetic stays inside the fresh block (`arrcap(asize) * 8` value bytes come first).
    unsafe {
        if arrcap(asize) != asize {
            // spare capacity (asize 1 or 2: a 4-slot block)
            let blk = heap
                .mem
                .alloc_counted(concretesize(arrcap(asize)), concretesize(asize))?;
            Ok(blk.as_ptr().add(arrcap(asize) as usize * 8))
        } else {
            Ok(heap
                .mem
                .alloc(concretesize(asize))?
                .as_ptr()
                .add(asize as usize * 8))
        }
    }
}

/// Free the block of an array part of `asize > 0` slots whose array pointer is `array`.
#[inline(always)]
unsafe fn free_array_block(heap: &mut Heap, array: *mut u8, asize: u32) {
    // SAFETY: T1: `array` is the end of the value slots of a block from `alloc_array` / `resizearray` for `asize` slots.
    unsafe {
        if arrcap(asize) != asize {
            let oc = arrcap(asize);
            heap.mem.free_counted(
                array.sub(oc as usize * 8),
                concretesize(oc),
                concretesize(asize),
            );
        } else {
            heap.mem
                .free(array.sub(asize as usize * 8), concretesize(asize));
        }
    }
}

/// Free the array part (`asize` slots, possibly spare capacity, see [`arrcap`]); `array` is dangling afterwards.
#[inline]
unsafe fn freearray(heap: &mut Heap, t: *mut Table) {
    // SAFETY: T1: `array` is null or the end of the value slots of a block from `resize_empty` / `resizearray`.
    unsafe {
        let asize = (*t).asize;
        if asize != 0 {
            free_array_block(heap, (*t).array, asize);
        }
    }
}

/// `resizearray`: returns the new `array` pointer (null for size 0).
unsafe fn resizearray(
    heap: &mut Heap,
    t: *mut Table,
    oldasize: u32,
    newasize: u32,
) -> Result<*mut u8, TableError> {
    // SAFETY: T1. The block of `oldasize` slots spans `arrcap(oldasize)` slots, starting at `array - 8 * arrcap`.
    unsafe {
        if oldasize == newasize {
            return Ok((*t).array);
        }
        let (oc, nc) = (arrcap(oldasize) as usize, arrcap(newasize) as usize);
        if newasize == 0 {
            freearray(heap, t);
            return Ok(std::ptr::null_mut());
        }
        if oldasize > 0 && nc == oc {
            // C22-E: the block already has room for the new size (growth 1 -> 2 -> 4, or the matching shrink): only the
            // accounted size changes
            heap.mem
                .recount(concretesize(oldasize), concretesize(newasize))?;
            return Ok((*t).array);
        }
        let newb = concretesize(nc as u32);
        if oldasize > 0 && newasize > oldasize && concretesize(oldasize) > 512 {
            // grow in place when the allocator can (large blocks are remapped, not copied):
            // layout is [values: asize*8][hint: 4][tags: asize], the array pointer being the end of
            // the values (capacity equals size for these big arrays)
            let (o, n) = (oldasize as usize, newasize as usize);
            let oldblk = (*t).array.sub(o * 8);
            let blk = match heap.mem.realloc(oldblk, concretesize(oldasize), newb)? {
                Some(b) => b.as_ptr(),
                None => return Err(TableError::Mem),
            };
            // hint + tags first (they move furthest), then the values up against the tags
            std::ptr::copy(blk.add(o * 8), blk.add(n * 8), 4 + o);
            std::ptr::copy(blk, blk.add((n - o) * 8), o * 8);
            return Ok(blk.add(n * 8));
        }
        let np = alloc_array(heap, newasize)?;
        if oldasize > 0 {
            let op = (*t).array;
            let tomove = oldasize.min(newasize) as usize;
            // values: the `tomove` slots just below the array pointer; hint+tags: `tomove + 4` bytes after it.
            std::ptr::copy_nonoverlapping(
                op.sub(tomove * 8),
                np.sub(tomove * 8),
                tomove * 8 + 4 + tomove,
            );
            free_array_block(heap, op, oldasize);
        }
        Ok(np)
    }
}

unsafe fn exchangehashpart(t1: *mut Table, t2: *mut Table) {
    // SAFETY: both valid tables.
    unsafe {
        let lsizenode = (*t1).lsizenode;
        let node = (*t1).node;
        let bd1 = (*t1).flags & BITDUMMY;
        (*t1).lsizenode = (*t2).lsizenode;
        (*t1).node = (*t2).node;
        (*t1).flags = ((*t1).flags & !BITDUMMY) | ((*t2).flags & BITDUMMY);
        (*t2).lsizenode = lsizenode;
        (*t2).node = node;
        (*t2).flags = ((*t2).flags & !BITDUMMY) | bd1;
    }
}

/// Temporary table header used by `resize` to hold a hash part.
fn blank_table() -> Table {
    Table {
        next: std::ptr::null_mut(),
        tt: LUA_VTABLE,
        marked: 0,
        flags: 0,
        lsizenode: 0,
        asize: 0,
        array: std::ptr::null_mut(),
        node: dummynode(),
        metatable: std::ptr::null_mut(),
        gclist: std::ptr::null_mut(),
    }
}

/// P-J: `luaH_resize` of a table with neither an array nor a hash part (a table just made by `new_table`): nothing to move,
/// no temporary table. The same allocations in the same order as the general path (node vector, then array; the node vector
/// is freed again when the array allocation fails), so the byte accounting and the memory-limit behaviour are identical.
#[inline]
unsafe fn resize_empty(
    heap: &mut Heap,
    t: *mut Table,
    newasize: u32,
    nhsize: u32,
) -> Result<(), TableError> {
    // SAFETY: `t` is a valid table with `asize == 0`, a null array and the dummy node (checked by the caller).
    unsafe {
        setnodevector(heap, t, nhsize)?;
        if newasize == 0 {
            return Ok(());
        }
        let np = match alloc_array(heap, newasize) {
            Ok(b) => b,
            Err(e) => {
                freehash(heap, t);
                (*t).node = dummynode();
                (*t).lsizenode = 0;
                (*t).flags |= BITDUMMY;
                return Err(e.into());
            }
        };
        (*t).array = np;
        (*t).asize = newasize;
        *lenhint(t) = if cfg!(rustynail_lua54) {
            newasize
        } else {
            newasize / 2
        };
        #[cfg(rustynail_lua54)]
        {
            (*t).flags &= !BITRAS;
        }
        for i in 0..newasize {
            *arr_tag(t, i as usize) = LUA_VEMPTY;
        }
        Ok(())
    }
}

/// `set_int` outside the array part, kept out of line so that the array fast path of `set_int` stays small enough to
/// inline into its callers (`jit_setlist`, the library). A table without a hash part has nothing to look up.
#[inline(never)]
unsafe fn set_int_hash(
    heap: &mut Heap,
    t: *mut Table,
    key: i64,
    value: &Value,
) -> Result<(), TableError> {
    // SAFETY: valid table.
    unsafe {
        let n = if isdummy(t) {
            std::ptr::null_mut()
        } else {
            getintfromhash(t, key)
        };
        if !n.is_null() {
            set_nval(n, value);
            Ok(())
        } else {
            newkey(heap, t, &Value::int(key), value)
        }
    }
}

// C22-C: switch for the differential tests (`tests_table.rs`): with it off every growth goes through the generic
// `resize` / counting path, so the specialised paths can be compared with it. Always on outside tests.
#[cfg(test)]
thread_local! {
    pub(crate) static NO_FAST_GROWTH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg_attr(rustynail_lua54, allow(dead_code))]
#[inline(always)]
fn fast_growth() -> bool {
    #[cfg(test)]
    {
        !NO_FAST_GROWTH.with(|c| c.get())
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// C22-C: `luaH_resize(t, asize, nhsize)` for the case that the array part keeps its size: the general `resize` without its
/// temporary table header and the array handling. Same observable result as the general path, which for `newasize ==
/// oldasize` does exactly: allocate the new node vector (before touching `t`: an allocation failure leaves `t` intact),
/// reset the length hint of a non-empty array part, reinsert the live nodes of the old hash part in node order, free the old
/// node vector. `newcheckedkey`'s array test is vacuous for those keys: an integer key in `1..=asize` is never in the hash
/// part (growing the array part moves such keys, every insert of one goes to the array), and the keys of dead nodes are
/// skipped. `nhsize` is at least the number of live nodes (the callers count them), so every reinsertion finds room.
#[cfg(not(rustynail_lua54))]
#[inline]
unsafe fn resize_hash(heap: &mut Heap, t: *mut Table, nhsize: u32) -> Result<(), TableError> {
    // SAFETY: valid table; `setnodevector` writes `node` / `lsizenode` / the dummy flag only after its allocation succeeded;
    // the old node vector is read and freed after, with the (`node`, `lsizenode`) pair saved before.
    unsafe {
        if !fast_growth() {
            return resize(heap, t, (*t).asize, nhsize);
        }
        let oldnode = (*t).node;
        let oldl = (*t).lsizenode;
        let olddummy = isdummy(t);
        setnodevector(heap, t, nhsize)?;
        if (*t).asize > 0 {
            *lenhint(t) = (*t).asize / 2;
        }
        if !olddummy {
            for j in 0..(1usize << oldl) {
                let old = oldnode.add(j);
                if !nval_empty(old) {
                    let done = insertkey(t, &nkey(old), &nval(old));
                    debug_assert!(done);
                    let _ = done;
                }
            }
            let extra = if sized_haslastfree(oldl) { LIMBOX } else { 0 };
            heap.mem
                .free(oldnode.cast::<u8>().sub(extra), sizehash_bytes(oldl));
        }
        Ok(())
    }
}

/// `luaH_resize`.
unsafe fn resize(
    heap: &mut Heap,
    t: *mut Table,
    newasize: u32,
    nhsize: u32,
) -> Result<(), TableError> {
    // SAFETY: T1/T2; follows the reference's order of operations so failures leave `t` intact.
    unsafe {
        let oldasize = (*t).asize;
        if newasize as u64 > MAXASIZE {
            return Err(TableError::Overflow);
        }
        if oldasize == 0 && isdummy(t) {
            return resize_empty(heap, t, newasize, nhsize);
        }
        if !cfg!(rustynail_lua54) && nhsize == 0 && isdummy(t) && newasize > oldasize {
            // bf: array growth of a table without a hash part: no temporary hash part, nothing to reinsert
            let newarray = resizearray(heap, t, oldasize, newasize)?;
            (*t).array = newarray;
            (*t).asize = newasize;
            *lenhint(t) = newasize / 2;
            for i in oldasize..newasize {
                *arr_tag(t, i as usize) = LUA_VEMPTY;
            }
            return Ok(());
        }
        let mut newt = blank_table();
        let pnewt: *mut Table = &mut newt;
        setnodevector(heap, pnewt, nhsize)?;
        if newasize < oldasize {
            exchangehashpart(t, pnewt);
            for i in newasize..oldasize {
                if !arr_empty(t, i as usize) {
                    let key = Value::int(i as i64 + 1);
                    let aux = arr_get(t, i as usize);
                    insertkey(t, &key, &aux);
                }
            }
            exchangehashpart(t, pnewt);
        }
        let newarray = match resizearray(heap, t, oldasize, newasize) {
            Ok(a) => a,
            Err(e) => {
                freehash(heap, pnewt);
                return Err(e);
            }
        };
        exchangehashpart(t, pnewt);
        (*t).array = newarray;
        (*t).asize = newasize;
        if !newarray.is_null() {
            // (5.4: `alimit = newasize`, the real size)
            *lenhint(t) = if cfg!(rustynail_lua54) {
                newasize
            } else {
                newasize / 2
            };
        }
        #[cfg(rustynail_lua54)]
        {
            (*t).flags &= !BITRAS;
        }
        for i in oldasize..newasize {
            *arr_tag(t, i as usize) = LUA_VEMPTY;
        }
        // reinsert elements from the old hash part (now in `newt`)
        let size = sizenode(pnewt);
        for j in 0..size {
            let old = gnode(pnewt, j);
            if !nval_empty(old) {
                let k = nkey(old);
                let v = nval(old);
                newcheckedkey(t, &k, &v);
            }
        }
        freehash(heap, pnewt);
        Ok(())
    }
}

/// 5.4 `rehash`: always counts the array part, no extra room for deleted entries.
#[cfg(rustynail_lua54)]
unsafe fn rehash(heap: &mut Heap, t: *mut Table, ek: &Value) -> Result<(), TableError> {
    // SAFETY: valid table.
    unsafe {
        let mut ct = Counters {
            total: 0,
            na: 0,
            deleted: false,
            nums: [0; MAXABITS as usize + 1],
        };
        setlimittosize(t);
        numusearray(t, &mut ct); // count keys in array part
        numusehash(t, &mut ct); // count keys in hash part
        if let Some(i) = ek.as_int() {
            countint(i, &mut ct); // count extra key
        }
        ct.total += 1;
        let asize = computesizes(&mut ct);
        resize(heap, t, asize, ct.total - ct.na)
    }
}

/// bf: `rehash` of a table without a hash part for an integer key in the array range: the same counts as the general
/// path (the dummy node counts as a deleted entry there too), filled by plain stores.
#[cfg(not(rustynail_lua54))]
#[inline(always)]
unsafe fn rehash_dummy_int(heap: &mut Heap, t: *mut Table, k: u32) -> Result<(), TableError> {
    // SAFETY: valid table with the dummy node.
    unsafe {
        let mut ct = Counters {
            total: 1,
            na: 0,
            deleted: true,
            nums: [0; MAXABITS as usize + 1],
        };
        let asize = (*t).asize;
        let mut ause = 0u32;
        let mut i = 1u32;
        let mut ttlg: u32 = 1;
        let mut lg = 0usize;
        while i <= asize {
            let lim = ttlg.min(asize);
            let mut lc = 0u32;
            while i <= lim {
                lc += (!arr_empty(t, i as usize - 1)) as u32;
                i += 1;
            }
            ct.nums[lg] = lc;
            ause += lc;
            ttlg = ttlg.wrapping_mul(2);
            lg += 1;
        }
        ct.nums[ceillog2(k) as usize] += 1;
        ct.total += ause;
        ct.na = ause + 1;
        let nasize = computesizes(&mut ct);
        let mut nsize = ct.total - ct.na;
        nsize += nsize >> 2;
        resize(heap, t, nasize, nsize)
    }
}

/// Is every slot of the array part non-nil (`asize` tag bytes at `array + 4`, 8 at a time)?
#[cfg(not(rustynail_lua54))]
#[inline]
unsafe fn array_dense(t: *const Table) -> bool {
    // SAFETY: T1: the `asize` tag bytes follow the hint.
    unsafe {
        let n = (*t).asize as usize;
        if n == 0 {
            return true; // (the array pointer is null: no offset)
        }
        let tags = (*t).array.add(4);
        let mut i = 0usize;
        while i + 8 <= n {
            let v = tags.add(i).cast::<u64>().read_unaligned() & 0x0F0F_0F0F_0F0F_0F0F;
            // some byte is zero (a nil tag: `tag & 0x0F == LUA_TNIL`)
            if v.wrapping_sub(0x0101_0101_0101_0101) & !v & 0x8080_8080_8080_8080 != 0 {
                return false;
            }
            i += 8;
        }
        while i < n {
            if *tags.add(i) & 0x0F == LUA_TNIL {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// C22-E2: `rehash` for the append `t[asize + 1] = v` (`k == asize + 1`, `1 <= k <= 2^31`) of a table whose array part is full
/// (no nil slot) and whose hash part holds no integer key. The generic count then is: `k` plus the `asize` array entries all
/// below `k`, no integer in the hash part, so `na = k` and `nums[i]` is the number of those in `(2^(i-1), 2^i]`;
/// `computesizes` walks `twotoi = 1, 2, ...` while `twotoi <= 3 * na` (always true up to `P = 2^ceillog2(k) < 2k <= 3k`) and
/// moves `optimal` to every `twotoi` whose class `nums[i]` is non-zero and with `twotoi <= 3 * a` (`a` = entries `<= twotoi`):
/// it is true at `P` (class of `k`, non-empty; `a = k`) and nothing lies above `P`, so the new array size is `P` and `na`
/// stays `k` (`computesizes` sets it to the `a` at the optimum). The hash part then gets `total - na = live nodes` entries
/// (`total = 1 + asize + live`), plus a quarter when a node with an empty value was seen (the dummy node counts as one). No
/// `Counters`, no `nums`: the same `resize(P, nsize)`. `None`: the preconditions do not hold, use the generic count.
#[cfg(not(rustynail_lua54))]
#[inline]
unsafe fn rehash_append(heap: &mut Heap, t: *mut Table, k: u32) -> Option<Result<(), TableError>> {
    // SAFETY: valid table.
    unsafe {
        let mut live = 0u32;
        let mut deleted = false;
        let mut i = sizenode(t);
        while i > 0 {
            i -= 1;
            let n = gnode(t, i);
            if nval_empty(n) {
                deleted = true;
            } else {
                if (*n).key_tt == LUA_VNUMINT {
                    return None;
                }
                live += 1;
            }
        }
        if !array_dense(t) {
            return None;
        }
        let mut nsize = live;
        if deleted {
            nsize += nsize >> 2;
        }
        Some(resize(heap, t, 1u32 << ceillog2(k), nsize))
    }
}

#[cfg(not(rustynail_lua54))]
unsafe fn rehash(heap: &mut Heap, t: *mut Table, ek: &Value) -> Result<(), TableError> {
    // SAFETY: valid table.
    unsafe {
        if let Some(i) = ek.as_int() {
            let k = arrayindex(i);
            if k != 0 {
                if k == (*t).asize.wrapping_add(1) && fast_growth() {
                    if let Some(r) = rehash_append(heap, t, k) {
                        return r;
                    }
                }
                if isdummy(t) {
                    return rehash_dummy_int(heap, t, k);
                }
            }
        }
        if ek.as_int().is_none() {
            // P-TINS4: no integer key in the hash part and none being inserted: `na` stays 0, the array part keeps
            // its size and only the live/deleted count of the nodes matters (no `Counters`, no zeroed `nums`)
            let mut total = 1u32;
            let mut deleted = false;
            let mut any_int = false;
            let mut i = sizenode(t);
            while i > 0 {
                i -= 1;
                let n = gnode(t, i);
                if nval_empty(n) {
                    deleted = true;
                } else {
                    total += 1;
                    any_int |= (*n).key_tt == LUA_VNUMINT;
                }
            }
            if !any_int {
                let mut nsize = total;
                if deleted {
                    nsize += nsize >> 2;
                }
                return resize_hash(heap, t, nsize);
            }
        }
        let mut ct = Counters {
            total: 1,
            na: 0,
            deleted: false,
            nums: [0; MAXABITS as usize + 1],
        };
        if let Some(i) = ek.as_int() {
            countint(i, &mut ct);
        }
        numusehash(t, &mut ct);
        if ct.na == 0 {
            // the array part keeps its size
            let mut nsize = ct.total;
            if ct.deleted {
                nsize += nsize >> 2;
            }
            return resize_hash(heap, t, nsize);
        }
        numusearray(t, &mut ct);
        let asize = computesizes(&mut ct);
        let mut nsize = ct.total - ct.na;
        if ct.deleted {
            nsize += nsize >> 2;
        }
        resize(heap, t, asize, nsize)
    }
}

unsafe fn newkey(
    heap: &mut Heap,
    t: *mut Table,
    key: &Value,
    value: &Value,
) -> Result<(), TableError> {
    if !value.is_nil() {
        // SAFETY: valid table.
        unsafe {
            if isdummy(t) || !insertkey(t, key, value) {
                rehash(heap, t, key)?;
                newcheckedkey(t, key, value);
            }
        }
        heap.barrier_back(t.cast(), key);
        heap.barrier_back(t.cast(), value);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// public API
// ---------------------------------------------------------------------------------------------

impl Heap {
    /// `luaH_new`.
    pub fn new_table(&mut self) -> Result<TableRef, Status> {
        let raw = self.mem.alloc(std::mem::size_of::<Table>())?;
        let p = raw.as_ptr().cast::<Table>();
        // SAFETY: fresh block big enough for a `Table`, 16-aligned; fully initialised before linking.
        unsafe {
            p.write(Table {
                next: self.allgc,
                tt: LUA_VTABLE,
                marked: self.current_white,
                flags: MASKFLAGS | BITDUMMY,
                lsizenode: 0,
                asize: 0,
                array: std::ptr::null_mut(),
                node: dummynode(),
                metatable: std::ptr::null_mut(),
                gclist: std::ptr::null_mut(),
            });
            self.allgc = p.cast();
            Ok(TableRef::from_raw(p))
        }
    }

    /// `luaH_free` (the object must already be unlinked from every list).
    ///
    /// # Safety
    /// `t` must be a live table of this heap; it is dead afterwards.
    pub(crate) unsafe fn free_table(&mut self, t: *mut Table) {
        // SAFETY: T1/T2; object allocated with `size_of::<Table>()`.
        unsafe {
            freehash(self, t);
            freearray(self, t);
            self.mem.free(t.cast(), std::mem::size_of::<Table>());
        }
    }
}

impl Table {
    /// `luaH_size`: bytes used by the table (object + array + hash).
    pub fn byte_size(&self) -> usize {
        let mut sz = std::mem::size_of::<Table>() + concretesize(self.asize);
        if self.flags & BITDUMMY == 0 {
            sz += sizehash_bytes(self.lsizenode);
        }
        sz
    }
}

impl TableRef {
    // ---- inspection ---------------------------------------------------------------------------

    /// Size of the array part.
    pub fn array_size(self) -> u32 {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).asize }
    }
    /// Number of hash nodes allocated (0 for the dummy node).
    pub fn hash_size(self) -> usize {
        // SAFETY: handle invariant.
        unsafe {
            if isdummy(self.as_ptr()) {
                0
            } else {
                sizenode(self.as_ptr())
            }
        }
    }
    pub fn metatable(self) -> Option<TableRef> {
        // SAFETY: handle invariant; metatable null or live.
        unsafe {
            let m = (*self.as_ptr()).metatable;
            if m.is_null() {
                None
            } else {
                Some(TableRef::from_raw(m))
            }
        }
    }
    /// Set the metatable (the caller handles GC barriers and `__gc` registration).
    pub fn set_metatable(self, mt: Option<TableRef>) {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).metatable = mt.map_or(std::ptr::null_mut(), TableRef::as_ptr) }
    }
    /// 5.4 `alimit`: the array limit kept in the hint slot (0 without an array part). Read-only; the trace
    /// recorder's lockstep model compares it with its own `ATOUCH` bookkeeping (docs/JIT_TRACE.md TR30).
    #[cfg(rustynail_lua54)]
    pub fn alimit_hint(self) -> u32 {
        // SAFETY: handle invariant; `alimit` checks `asize` (T1).
        unsafe { alimit(self.as_ptr()) }
    }
    /// log2 of the node-vector size (0 for the shared dummy node, whose single node holds a dead key).
    /// Read-only; used by the trace recorder (docs/JIT_TRACE.md TR13.4, task F1).
    pub fn lsizenode(self) -> u8 {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).lsizenode }
    }
    /// Index of the node that holds the short-string key `key` (a live interned string), `None` when the
    /// hash part has no such key. Read-only (the recorder's `HREFK` slot index, task F1).
    pub fn short_str_node(self, key: StrRef) -> Option<usize> {
        // SAFETY: handle invariants; `key` is a live short string; the found node lies in the node vector.
        unsafe {
            let t = self.as_ptr();
            let n = hgetshortstr(t, key.as_ptr());
            if n.is_null() {
                None
            } else {
                Some(n.offset_from((*t).node) as usize)
            }
        }
    }
    /// H1: index of the node that holds the integer key `key` in the hash part (`getintfromhash`), `None` when the key is in the
    /// array part or absent from the hash part. Read-only (the trace compiler's `HREFV` integer form).
    pub fn int_node(self, key: i64) -> Option<usize> {
        // SAFETY: handle invariants; the node found lies in the node vector.
        unsafe {
            let t = self.as_ptr();
            if ikeyinarray(t, key) > 0 {
                return None;
            }
            let n = getintfromhash(t, key);
            if n.is_null() {
                None
            } else {
                Some(n.offset_from((*t).node) as usize)
            }
        }
    }
    /// Index of the main position of the short-string key `key` (a live interned string): where `luaH_getshortstr`
    /// starts its chain walk. Read-only (P-A `HMISS` absent-key proof).
    pub fn short_str_mainpos(self, key: StrRef) -> usize {
        // SAFETY: handle invariants; `key` is a live short string; the main position lies in the node vector.
        unsafe {
            let t = self.as_ptr();
            let h = if crate::string::mut_hash(crate::string::MUT_RECORD_MAINPOS_RAW) {
                (*key.as_ptr()).short_hash_raw()
            } else {
                LuaString::short_hash(key.as_ptr())
            };
            hashpow2(t, h).offset_from((*t).node) as usize
        }
    }
    /// A1: where `insertkey` would put the short-string key `key` (a live interned string, **absent** from the table) and what
    /// it would change, for the trace compiler's inline `NEWREF`. Read-only. `None` when the insertion is not one of the two
    /// simple shapes (the key goes to its free main position, or the colliding node sits in its own main position and the
    /// new key goes to the free node `getfreepos` finds), when the hash part is the dummy node, has more than 8 nodes, holds a
    /// key that is not a short string (or a node that is not in the canonical state of a table built from nothing:
    /// a free node with a chain link), or needs a rehash. The plan is a function of the node vector alone (keys, chain links,
    /// which values are empty, the `lastfree` position), which is what makes it valid for every table that went through the same
    /// sequence of insertions (A1 / docs/JIT_TRACE_REF.md).
    #[cold]
    #[inline(never)]
    pub fn plan_shortstr_insert(self, key: StrRef) -> Option<InsertPlan> {
        // SAFETY: handle invariants; `key` is a live short string; every node touched lies in the node vector (T2/T3).
        unsafe {
            let t = self.as_ptr();
            if isdummy(t) || sizenode(t) > 8 {
                return None;
            }
            for i in 0..sizenode(t) {
                let n = gnode(t, i);
                let kt = (*n).key_tt;
                if kt != LUA_TNIL && kt != ctb(LUA_VSHRSTR) {
                    return None;
                }
                if kt == LUA_TNIL && (*n).next != 0 {
                    return None;
                }
            }
            if !hgetshortstr(t, key.as_ptr()).is_null() {
                return None;
            }
            let kv = Value::string(key);
            let mp = mainposition(t, &kv);
            let flags = (*t).flags & !MASKFLAGS;
            let mpi = mp.offset_from((*t).node) as u8;
            if !nval_empty(mp) || isdummy(t) {
                let mut f: *mut Node = std::ptr::null_mut();
                let mut last = 0usize;
                if haslastfree(t) {
                    let mut p = *lastfree_slot(t);
                    while p > (*t).node {
                        p = p.sub(1);
                        if key_is_nil(p) {
                            f = p;
                            break;
                        }
                    }
                } else {
                    let mut i = sizenode(t);
                    while i > 0 {
                        i -= 1;
                        if key_is_nil(gnode(t, i)) {
                            f = gnode(t, i);
                            break;
                        }
                    }
                }
                if f.is_null() || (*f).next != 0 {
                    return None;
                }
                let fi = f.offset_from((*t).node) as usize;
                if haslastfree(t) {
                    last = fi;
                }
                let othern = mainposition(t, &nkey(mp));
                if othern != mp {
                    return None; // the colliding node is out of its main position: it would be moved
                }
                let node_next = if (*mp).next != 0 {
                    mp.offset((*mp).next as isize).offset_from(f) as i32
                } else {
                    0
                };
                return Some(InsertPlan {
                    node: fi as u8,
                    main: mpi,
                    main_next: f.offset_from(mp) as i32,
                    node_next,
                    lastfree: if haslastfree(t) {
                        Some(last as u8)
                    } else {
                        None
                    },
                    flags,
                });
            }
            if !key_is_nil(mp) {
                return None; // an empty value behind a key (not canonical)
            }
            Some(InsertPlan {
                node: mpi,
                main: mpi,
                main_next: (*mp).next,
                node_next: (*mp).next,
                lastfree: None,
                flags,
            })
        }
    }
    /// A1: the hash part as the trace recorder compares it to the state it expects: per node `(key tag, key payload, chain
    /// link, value is empty)`, and the index `lastfree` points at (0 without one). Read-only.
    #[cold]
    #[inline(never)]
    pub fn hash_image(self) -> HashImage {
        // SAFETY: handle invariant; nodes lie in the node vector (T2).
        unsafe {
            let t = self.as_ptr();
            let n = if isdummy(t) { 0 } else { sizenode(t) };
            let img = (0..n)
                .map(|i| {
                    let nd = gnode(t, i);
                    (
                        (*nd).key_tt,
                        (*nd).key.addr() as u64,
                        (*nd).next,
                        nval_empty(nd),
                    )
                })
                .collect();
            let last = if n > 0 && haslastfree(t) {
                (*lastfree_slot(t)).offset_from((*t).node) as usize
            } else {
                0
            };
            (img, last)
        }
    }
    /// Chain offset (`next`, in nodes) of node `idx` (`idx < 2^lsizenode`). Read-only (P-A `HMISS`).
    pub fn node_next(self, idx: usize) -> i32 {
        // SAFETY: handle invariant; `idx` is checked against the node count (T2).
        unsafe {
            let t = self.as_ptr();
            assert!(idx < sizenode(t), "node index out of range");
            (*gnode(t, idx)).next
        }
    }
    /// The trace compiler's `NEWREF`: insert the short-string key `key` (a live interned string) with the value `true`
    /// and return the index of the node that now holds it. `None` when the key is already in the hash part (also with an
    /// empty value) and `present_ok` is false (nothing done), or when the insertion fails (memory limit: the table is intact).
    /// Same result as `short_str_node` + `set_str` + `short_str_node`, with one lookup for the presence test and one for
    /// the answer instead of four (P-TINS3). Does what `set` does: rehash if needed, write barrier, TM cache invalidation.
    pub fn newref_shortstr(self, heap: &mut Heap, key: StrRef, present_ok: bool) -> Option<usize> {
        // SAFETY: handle invariants; `key` is a live short string (T3/T4); the node found lies in the node vector.
        unsafe {
            let t = self.as_ptr();
            if !hgetshortstr(t, key.as_ptr()).is_null() {
                if !present_ok {
                    return None;
                }
                self.set_str(heap, key, &Value::TRUE).ok()?;
            } else {
                newkey(heap, t, &Value::string(key), &Value::TRUE).ok()?;
                self.invalidate_tm_cache();
            }
            let n = hgetshortstr(t, key.as_ptr());
            if n.is_null() {
                None
            } else {
                Some(n.offset_from((*t).node) as usize)
            }
        }
    }
    /// Address of node `idx` (`idx < 2^lsizenode`). Read-only (J5 `NEWREF` hands it to the trace).
    pub fn node_addr(self, idx: usize) -> usize {
        // SAFETY: handle invariant; `idx` is checked against the node count (T2).
        unsafe {
            let t = self.as_ptr();
            assert!(idx < sizenode(t), "node index out of range");
            gnode(t, idx) as usize
        }
    }
    /// Key tag and key payload bits of node `idx` (`idx < 2^lsizenode`; the dummy node's key is a dead key).
    /// Read-only (task F1 lockstep model).
    pub fn node_key(self, idx: usize) -> (u8, u64) {
        // SAFETY: handle invariant; `idx` is checked against the node count (T2).
        unsafe {
            let t = self.as_ptr();
            assert!(idx < sizenode(t), "node index out of range");
            let n = gnode(t, idx);
            ((*n).key_tt, (*n).key.addr() as u64)
        }
    }
    /// The key of node `idx` as a pointer *with provenance* (F6: `node_key` returns the address as an integer, which Miri cannot turn
    /// back into a pointer). Meaningful only when the node's key tag is a collectable type; read-only, dereferenced by the caller only.
    pub fn node_key_gc(self, idx: usize) -> *mut GcHeader {
        // SAFETY: handle invariant; `idx` is checked against the node count (T2); the payload word is read as a pointer, which is
        // a plain copy of the stored bits (the caller checks the key tag before using it).
        unsafe {
            let t = self.as_ptr();
            assert!(idx < sizenode(t), "node index out of range");
            (*gnode(t, idx)).key.gc()
        }
    }
    /// Value of node `idx` as stored (nil variants included). Read-only (task F1 lockstep model).
    pub fn node_value(self, idx: usize) -> Value {
        // SAFETY: handle invariant; `idx` is checked against the node count (T2).
        unsafe {
            let t = self.as_ptr();
            assert!(idx < sizenode(t), "node index out of range");
            nval(gnode(t, idx))
        }
    }
    /// Metamethod-absence cache flags.
    pub fn flags(self) -> u8 {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).flags }
    }
    /// `invalidateTMcache`.
    #[inline]
    pub fn invalidate_tm_cache(self) {
        // SAFETY: handle invariant.
        unsafe { (*self.as_ptr()).flags &= !MASKFLAGS }
    }
    /// `checknoTM`: true when the (optional) metatable is known not to have event `e` (fast events only).
    #[inline]
    pub fn no_tm(mt: Option<TableRef>, e: Tm) -> bool {
        match mt {
            None => true,
            Some(m) => m.flags() & (1u8 << e as u8) != 0,
        }
    }

    // ---- reads --------------------------------------------------------------------------------

    /// `luaH_getint`; absent and empty both give nil.
    #[inline]
    pub fn get_int(self, key: i64) -> Value {
        let t = self.as_ptr();
        // SAFETY: handle invariant; index checked against asize.
        unsafe {
            let k = ikeyinarray(t, key);
            if k > 0 {
                alimit_touch(t, k as u64 - 1);
                let v = arr_get(t, k as usize - 1);
                if v.is_nil() {
                    Value::NIL
                } else {
                    v
                }
            } else {
                node_value_or_nil(getintfromhash(t, key))
            }
        }
    }

    /// [`get_int_arr`](Self::get_int_arr) without the 5.4 `alimit_touch` side effect: for observers (the trace recorder's lockstep and
    /// its look-ahead reads) that must not move the array limit, which `#t` of a table with holes depends on. Identical on 5.5.
    #[inline(always)]
    pub fn peek_int_arr(self, key: i64) -> Option<Value> {
        let t = self.as_ptr();
        let u = (key as u64).wrapping_sub(1);
        // SAFETY: handle invariant; index checked against asize.
        unsafe {
            if u < (*t).asize as u64 {
                let tag = *arr_tag(t, u as usize);
                if tag & 0x0F != LUA_TNIL {
                    return Some(Value::from_parts(tag, *arr_val(t, u as usize)));
                }
            }
        }
        None
    }
    /// Array-part hit of `get_int`: `Some` only for a non-nil value in the array part.
    #[inline(always)]
    pub fn get_int_arr(self, key: i64) -> Option<Value> {
        let t = self.as_ptr();
        let u = (key as u64).wrapping_sub(1);
        // SAFETY: handle invariant; index checked against asize.
        unsafe {
            if u < (*t).asize as u64 {
                alimit_touch(t, u);
                let tag = *arr_tag(t, u as usize);
                if tag & 0x0F != LUA_TNIL {
                    return Some(Value::from_parts(tag, *arr_val(t, u as usize)));
                }
            }
        }
        None
    }

    /// `luaH_getshortstr`.
    #[inline]
    pub fn get_short_str(self, key: StrRef) -> Value {
        // SAFETY: handle invariants.
        unsafe { node_value_or_nil(hgetshortstr(self.as_ptr(), key.as_ptr())) }
    }

    /// `luaH_getstr` (short or long).
    pub fn get_str(self, key: StrRef) -> Value {
        // SAFETY: handle invariants.
        unsafe { node_value_or_nil(hgetstr(self.as_ptr(), key.as_ptr())) }
    }

    /// [`TableRef::get`] with the key passed by value (in registers): the interpreter's fast
    /// paths use it so that their operand copies are never address-taken (spilled).
    #[inline(never)]
    pub fn get_by_val(self, key: Value) -> Value {
        self.get(&key)
    }

    /// `luaH_get`: raw get; absent keys (including nil/NaN) give nil.
    pub fn get(self, key: &Value) -> Value {
        let t = self.as_ptr();
        match key.tag() {
            x if x == ctb(LUA_VSHRSTR) => match key.as_gc() {
                // SAFETY: live short string; the pointer keeps the full provenance of the value's payload.
                Some(s) => unsafe { node_value_or_nil(hgetshortstr(t, s.cast())) },
                None => Value::NIL,
            },
            LUA_VNUMINT => self.get_int(key.as_int().unwrap_or(0)),
            LUA_VNIL | LUA_VEMPTY | LUA_VABSTKEY | LUA_VNOTABLE => Value::NIL,
            LUA_VNUMFLT => {
                let f = key.as_float().unwrap_or(0.0);
                match float_to_int(f, F2I::Eq) {
                    Some(k) => self.get_int(k),
                    // SAFETY: valid table.
                    None => unsafe { node_value_or_nil(getgeneric(t, key, false)) },
                }
            }
            // SAFETY: valid table.
            _ => unsafe { node_value_or_nil(getgeneric(t, key, false)) },
        }
    }

    // ---- writes -------------------------------------------------------------------------------

    /// `luaH_pset`: set if the key is present with a non-nil value; otherwise an encoding of where it
    /// was (see `HOK`..): `HNOTFOUND`, `HFIRSTNODE + node index`, or `~array index` (negative).
    pub fn pset(self, key: Value, val: Value) -> i32 {
        let t = self.as_ptr();
        match key.tag() {
            x if x == ctb(LUA_VSHRSTR) => match key.as_gc() {
                // SAFETY: live short string; payload pointer keeps full provenance (it may be stored as a key).
                Some(s) => unsafe { self.psetshortstr(s.cast(), val) },
                None => HNOTFOUND,
            },
            LUA_VNUMINT => self.psetint(key.as_int().unwrap_or(0), val),
            LUA_VNIL => HNOTFOUND,
            LUA_VNUMFLT => {
                let f = key.as_float().unwrap_or(0.0);
                match float_to_int(f, F2I::Eq) {
                    Some(k) => self.psetint(k, val),
                    // SAFETY: valid table.
                    None => unsafe { finishnodeset(t, getgeneric(t, &key, false), &val) },
                }
            }
            // SAFETY: valid table.
            _ => unsafe { finishnodeset(t, getgeneric(t, &key, false), &val) },
        }
    }

    /// Inline array-part store over a non-empty slot, or over an empty one of a table without
    /// a metatable (no `__newindex` to consult: filling a pre-sized array); `false` means "use
    /// [`TableRef::psetint`]".
    #[inline(always)]
    pub fn pset_existing_int_arr(self, key: i64, val: Value) -> bool {
        let t = self.as_ptr();
        let u = (key as u64).wrapping_sub(1);
        // SAFETY: handle invariant; index checked against asize.
        unsafe {
            if u < (*t).asize as u64 && (!arr_empty(t, u as usize) || (*t).metatable.is_null()) {
                alimit_touch(t, u);
                arr_set(t, u as usize, &val);
                return true;
            }
        }
        false
    }

    /// N2: bulk `t[dst + j] = t[src + j]` for `j` in `0..count` (the library's element-shifting
    /// loops of `table.insert/remove/move`, `memmove` semantics: the ranges may overlap).
    ///
    /// Taken only when both ranges lie entirely inside the array part (1-based indices, `src`,
    /// `dst` >= 1) and no metamethod can fire for these reads and writes (no metatable, or one
    /// known to lack `__index` and `__newindex`); otherwise nothing is touched and `false` is
    /// returned, so the caller runs the exact `lua_geti`/`lua_seti` loop.
    ///
    /// No GC barrier is needed: the table keeps exactly the children it had (a value only moves
    /// to another slot of the same table), so a black table cannot gain a white child.
    /// `backward` is the direction of the reference loop; it only matters for the 5.4 array-limit
    /// hint, which every `luaH_getint` / `luaH_setint` access updates.
    pub fn arr_move(self, src: i64, dst: i64, count: usize, backward: bool) -> bool {
        let t = self.as_ptr();
        let mt = self.metatable();
        if !mut_tbl_move_meta_skip()
            && !(TableRef::no_tm(mt, Tm::Index) && TableRef::no_tm(mt, Tm::NewIndex))
        {
            return false;
        }
        if src < 1 || dst < 1 {
            return false;
        }
        // SAFETY: handle invariant; both ranges are checked against asize before any access.
        unsafe {
            let asize = (*t).asize as u64;
            let n = count as u64;
            if (src as u64 - 1) + n > asize || (dst as u64 - 1) + n > asize {
                return false;
            }
            if n == 0 {
                return true;
            }
            #[cfg(rustynail_lua54)]
            {
                // replay the limit updates of the reference's alternating read / write accesses
                let (s, d) = (src as u64 - 1, dst as u64 - 1);
                if backward {
                    for j in (0..n).rev() {
                        alimit_touch(t, s + j);
                        alimit_touch(t, d + j);
                    }
                } else {
                    for j in 0..n {
                        alimit_touch(t, s + j);
                        alimit_touch(t, d + j);
                    }
                }
            }
            #[cfg(not(rustynail_lua54))]
            let _ = backward;
            let (s, d) = (src as usize - 1, dst as usize - 1);
            let c = count;
            // tags ascend with the index, payloads descend: the range [s, s + c) of payloads
            // starts at `arr_val(s + c - 1)`
            std::ptr::copy(arr_tag(t, s), arr_tag(t, d), c);
            std::ptr::copy(arr_val(t, s + c - 1), arr_val(t, d + c - 1), c);
        }
        true
    }

    /// N2: bulk `dst[dpos + j] = self[spos + j]` for `j` in `0..count` between two *different*
    /// tables (`table.move(a, f, e, t, b)`). Taken only when the source range lies in `self`'s
    /// array part, the destination range in `dst`'s, `self` has no `__index` and `dst` no
    /// `__newindex` (no metatable, or one known to lack it); otherwise nothing is touched and
    /// `false` is returned.
    ///
    /// GC: `dst` gains children, so a black `dst` is handled like the per-element `raw_set_int`
    /// barrier would (`barrier_back` for every collectable value; the first white one turns the
    /// table gray, the rest are no-ops).
    pub fn arr_copy_to(
        self,
        heap: &mut Heap,
        dst: TableRef,
        spos: i64,
        dpos: i64,
        count: usize,
    ) -> bool {
        let (s, d) = (self.as_ptr(), dst.as_ptr());
        if s == d || spos < 1 || dpos < 1 {
            return false;
        }
        if !mut_tbl_move_meta_skip()
            && !(TableRef::no_tm(self.metatable(), Tm::Index)
                && TableRef::no_tm(dst.metatable(), Tm::NewIndex))
        {
            return false;
        }
        // SAFETY: handle invariants; both ranges are checked against the array sizes before any access.
        unsafe {
            let n = count as u64;
            if (spos as u64 - 1) + n > (*s).asize as u64
                || (dpos as u64 - 1) + n > (*d).asize as u64
            {
                return false;
            }
            if n == 0 {
                return true;
            }
            let (si, di) = (spos as usize - 1, dpos as usize - 1);
            #[cfg(rustynail_lua54)]
            for j in 0..count {
                alimit_touch(s, (si + j) as u64);
                alimit_touch(d, (di + j) as u64);
            }
            std::ptr::copy_nonoverlapping(arr_tag(s, si), arr_tag(d, di), count);
            std::ptr::copy_nonoverlapping(
                arr_val(s, si + count - 1),
                arr_val(d, di + count - 1),
                count,
            );
            if (*d).marked & BLACKBIT != 0 {
                for j in 0..count {
                    let v = arr_get(d, di + j);
                    heap.barrier_back(d.cast(), &v);
                }
            }
        }
        true
    }

    /// `psetint` with the array fast path (`luaH_fastseti`).
    pub fn psetint(self, key: i64, val: Value) -> i32 {
        let t = self.as_ptr();
        // SAFETY: handle invariant; array index checked.
        unsafe {
            let u = (key as u64).wrapping_sub(1);
            if u < (*t).asize as u64 {
                alimit_touch(t, u);
                let u = u as usize;
                if !arr_empty(t, u) || TableRef::no_tm(self.metatable(), Tm::NewIndex) {
                    arr_set(t, u, &val);
                    HOK
                } else {
                    !(u as i32)
                }
            } else {
                finishnodeset(t, getintfromhash(t, key), &val)
            }
        }
    }

    /// Inline fast path of `psetshortstr`: overwrite an existing non-empty entry. `false` means
    /// "use [`TableRef::psetshortstr`]" (nothing has been modified).
    ///
    /// # Safety
    /// `key` must be a live short string.
    #[inline(always)]
    pub unsafe fn pset_existing_shortstr(self, key: *const LuaString, val: Value) -> bool {
        let t = self.as_ptr();
        // SAFETY: per contract; T2/T3.
        unsafe {
            let slot = hgetshortstr(t, key);
            if !slot.is_null() && !nval_empty(slot) {
                set_nval(slot, &val);
                return true;
            }
        }
        false
    }

    /// [`TableRef::pset_existing_shortstr`] that hands the lookup's result to
    /// [`TableRef::psetshortstr_after`] on a miss, so that a store of a new key (or one going to
    /// `__newindex`) does not look the key up twice.
    ///
    /// # Safety
    /// `key` must be a live short string.
    #[inline(always)]
    pub unsafe fn pset_existing_shortstr_slot(
        self,
        key: *const LuaString,
        val: Value,
    ) -> Result<(), StrSlot> {
        let t = self.as_ptr();
        // SAFETY: per contract; T2/T3.
        unsafe {
            let slot = hgetshortstr(t, key);
            if !slot.is_null() && !nval_empty(slot) {
                set_nval(slot, &val);
                return Ok(());
            }
            Err(StrSlot(slot))
        }
    }

    /// `luaH_psetshortstr`.
    ///
    /// # Safety
    /// `key` must be a live short string.
    pub unsafe fn psetshortstr(self, key: *const LuaString, val: Value) -> i32 {
        let t = self.as_ptr();
        // SAFETY: per contract; T2/T3.
        unsafe {
            let slot = hgetshortstr(t, key);
            if !slot.is_null() && !nval_empty(slot) {
                set_nval(slot, &val);
                return HOK;
            }
            self.psetshortstr_miss(key, slot, val)
        }
    }

    /// The rest of [`TableRef::psetshortstr`] after [`TableRef::pset_existing_shortstr_slot`]
    /// found no existing entry: insert the key if there is no `__newindex`, else (or when the
    /// insertion needs a rehash or a barrier) return the code for `finish_set`.
    ///
    /// # Safety
    /// `key` must be a live short string and `slot` the result of that lookup on this table, with
    /// nothing modified in between.
    #[inline(never)]
    pub unsafe fn psetshortstr_after(
        self,
        key: *const LuaString,
        slot: StrSlot,
        val: Value,
    ) -> i32 {
        // SAFETY: per contract.
        unsafe { self.psetshortstr_miss(key, slot.0, val) }
    }

    /// # Safety
    /// As [`TableRef::psetshortstr_after`] (`slot` null or a node of this table).
    #[inline(always)]
    unsafe fn psetshortstr_miss(self, key: *const LuaString, slot: *mut Node, val: Value) -> i32 {
        let t = self.as_ptr();
        // SAFETY: per contract of the callers; T2/T3.
        unsafe {
            if TableRef::no_tm(self.metatable(), Tm::NewIndex) {
                if val.is_nil() {
                    return HOK;
                }
                let need_barrier =
                    (*t).marked & BLACKBIT != 0 && (*key).hdr.marked & WHITEBITS != 0;
                if slot.is_null() && !need_barrier {
                    let tk = Value::string(StrRef(NonNull::new_unchecked(key as *mut LuaString)));
                    if insertkey(t, &tk, &val) {
                        self.invalidate_tm_cache();
                        return HOK;
                    }
                }
            }
            retpsetcode(t, slot)
        }
    }

    /// `luaH_finishset`: complete a set after `pset` returned `hres != HOK`.
    pub fn finish_set(
        self,
        heap: &mut Heap,
        key: &Value,
        value: &Value,
        hres: i32,
    ) -> Result<(), TableError> {
        let t = self.as_ptr();
        debug_assert!(hres != HOK);
        // SAFETY: handle invariants; `hres` came from `pset` on this table.
        unsafe {
            if hres == HNOTFOUND {
                // not `*key`: the VM wrote the key as two 8-byte stores a moment ago; one 16-byte load of it fails
                // store-to-load forwarding (~12 cycles, 25-45 % of the whole insert on a fresh table: P-TINS1)
                let mut actk = Value::load(key);
                if key.is_nil() {
                    return Err(TableError::NilIndex);
                } else if let Some(f) = key.as_float() {
                    if let Some(k) = float_to_int(f, F2I::Eq) {
                        actk = Value::int(k);
                    } else if f.is_nan() {
                        return Err(TableError::NanIndex);
                    }
                } else if key.is_long_string() {
                    // an external string may be short: it must be internalised to be used as a key
                    if let (Some(s), Some(g)) = (key.as_string(), key.as_gc()) {
                        if s.len() <= crate::string::MAX_SHORT_LEN {
                            let sr = StrRef(NonNull::new_unchecked(g.cast::<LuaString>()));
                            actk = Value::string(heap.normalize_str(sr)?);
                        }
                    }
                }
                newkey(heap, t, &actk, value)
            } else if hres > 0 {
                set_nval(gnode(t, (hres - HFIRSTNODE) as usize), value);
                Ok(())
            } else {
                arr_set(t, (!hres) as usize, value);
                Ok(())
            }
        }
    }

    /// `luaH_set` + TM-cache invalidation (raw set, no metamethods).
    pub fn set(self, heap: &mut Heap, key: &Value, value: &Value) -> Result<(), TableError> {
        // SAFETY: references point to initialised values (`Value::load`: no 16-byte load, see `finish_set`).
        let hres = unsafe { self.pset(Value::load(key), Value::load(value)) };
        let r = if hres != HOK {
            self.finish_set(heap, key, value, hres)
        } else {
            Ok(())
        };
        self.invalidate_tm_cache();
        r
    }

    /// `luaH_setint`.
    pub fn set_int(self, heap: &mut Heap, key: i64, value: &Value) -> Result<(), TableError> {
        let t = self.as_ptr();
        // SAFETY: handle invariants.
        unsafe {
            let ik = ikeyinarray(t, key);
            if ik > 0 {
                alimit_touch(t, ik as u64 - 1);
                arr_set(t, ik as usize - 1, value);
                return Ok(());
            }
            set_int_hash(heap, t, key, value)
        }
    }

    /// Raw set with a string key (interned or not).
    pub fn set_str(self, heap: &mut Heap, key: StrRef, value: &Value) -> Result<(), TableError> {
        self.set(heap, &Value::string(key), value)
    }

    /// `table.clear` (LuaJIT extension): sets every array slot and every hash value to nil, keeping the allocated array
    /// and hash capacity, the keys' node layout and the metatable. Raw (no metamethods). This is `t[k] = nil` for every
    /// present key, so it needs no write barrier (a nil store creates no reference) and leaves the shape that compiled
    /// code and traces guard on (array size, node count, key positions) untouched; entries are dead-keyed by the
    /// collector exactly as after individual removals, and a traversal in progress simply finds nothing left.
    pub fn clear_values(self) {
        let t = self.as_ptr();
        // SAFETY: handle invariant; every index is below `asize` / `sizenode`.
        unsafe {
            for i in 0..(*t).asize as usize {
                if !arr_empty(t, i) {
                    arr_set(t, i, &Value::NIL);
                }
            }
            if !isdummy(t) {
                for j in 0..sizenode(t) {
                    let n = gnode(t, j);
                    if !nval_empty(n) {
                        set_nval(n, &Value::NIL);
                    }
                }
            }
        }
        self.invalidate_tm_cache();
    }

    /// `luaH_resize`.
    pub fn resize(self, heap: &mut Heap, nasize: u32, nhsize: u32) -> Result<(), TableError> {
        // SAFETY: handle invariant.
        unsafe { resize(heap, self.as_ptr(), nasize, nhsize) }
    }

    /// `luaH_resizearray`.
    pub fn resize_array(self, heap: &mut Heap, nasize: u32) -> Result<(), TableError> {
        let nsize = self.hash_size() as u32;
        self.resize(heap, nasize, nsize)
    }

    /// Debugging aid: (index of the main position of `key`, node count, index of the node holding
    /// `key` if any, whether that main position currently holds a live value).
    #[doc(hidden)]
    pub fn debug_position(self, key: &Value) -> (usize, usize, Option<usize>, bool) {
        let t = self.as_ptr();
        // SAFETY: handle invariant.
        unsafe {
            let mp = mainposition(t, key);
            let n = getgeneric(t, key, false);
            (
                mp.offset_from((*t).node) as usize,
                sizenode(t),
                if n.is_null() {
                    None
                } else {
                    Some(n.offset_from((*t).node) as usize)
                },
                !nval_empty(mp),
            )
        }
    }

    /// Debugging aid: one line per hash node: `idx: E=<empty> next=<n> K=<key>`.
    #[doc(hidden)]
    pub fn debug_nodes(self) -> Vec<String> {
        let t = self.as_ptr();
        let mut out = vec![format!(
            "ASIZE {} NODES {}",
            self.array_size(),
            self.hash_size()
        )];
        // SAFETY: handle invariant.
        unsafe {
            if isdummy(t) {
                return out;
            }
            for i in 0..sizenode(t) {
                let n = gnode(t, i);
                let k = (*n).key_tt;
                let kd = if k == LUA_VNIL {
                    "nil".to_string()
                } else if k == LUA_TDEADKEY {
                    "dead".to_string()
                } else if k == LUA_VNUMINT {
                    format!("i{}", (*n).key.int())
                } else if k == LUA_VNUMFLT {
                    "fl".to_string()
                } else if let Some(s) = nkey(n).as_string() {
                    let b = s.as_bytes();
                    let head = String::from_utf8_lossy(&b[..b.len().min(12)]).into_owned();
                    if b.len() > 12 {
                        format!("s{head}{}", b[b.len() - 1] as char)
                    } else {
                        format!("s{head}")
                    }
                } else {
                    format!("?{k}")
                };
                out.push(format!(
                    "{i}: E={} next={} K={kd}",
                    nval_empty(n) as i32,
                    (*n).next
                ));
            }
        }
        out
    }

    // ---- length & traversal ---------------------------------------------------------------------

    /// 5.4 `luaH_getn`: a border of the table, using (and updating) the array limit `alimit`.
    #[cfg(rustynail_lua54)]
    pub fn len(self, heap: &Heap) -> u64 {
        let _ = heap;
        let t = self.as_ptr();
        // SAFETY: T1/T2; every index is checked against the real array size `asize`.
        unsafe {
            let asize = (*t).asize;
            let mut limit = alimit(t);
            if limit > 0 && arr_empty(t, limit as usize - 1) {
                // there must be a boundary before 'limit'
                if limit >= 2 && !arr_empty(t, limit as usize - 2) {
                    // 'limit - 1' is a boundary; can it be a new limit?
                    if ispow2realasize(t) && !(limit - 1).is_power_of_two() {
                        *lenhint(t) = limit - 1;
                        (*t).flags |= BITRAS; // now 'alimit' is not the real size
                    }
                    return (limit - 1) as u64;
                } else {
                    // must search for a boundary in [0, limit]
                    let boundary = binsearch(t, 0, limit);
                    // can this boundary represent the real size of the array?
                    if ispow2realasize(t) && boundary > asize / 2 {
                        *lenhint(t) = boundary;
                        (*t).flags |= BITRAS;
                    }
                    return boundary as u64;
                }
            }
            // 'limit' is zero or present in table
            if limit != asize {
                // 'limit' > 0 and array has more elements after 'limit'
                if arr_empty(t, limit as usize) {
                    return limit as u64; // 'limit + 1' is empty: this is the boundary
                }
                // else, try last element in the array
                limit = asize;
                if arr_empty(t, limit as usize - 1) {
                    // there must be a boundary in the array after old limit, and it must be a
                    // valid new limit
                    let boundary = binsearch(t, alimit(t), limit);
                    *lenhint(t) = boundary;
                    return boundary as u64;
                }
                // else, new limit is present in the table; check the hash part
            }
            if isdummy(t) || hashkeyisempty(t, limit as u64 + 1) {
                limit as u64 // 'limit + 1' is absent
            } else {
                hash_search54(t, limit as u64)
            }
        }
    }

    /// `luaH_getn`: a border of the table.
    #[cfg(not(rustynail_lua54))]
    pub fn len(self, heap: &Heap) -> u64 {
        let t = self.as_ptr();
        // SAFETY: T1/T2; indices checked against asize.
        unsafe {
            let asize = (*t).asize;
            if asize > 0 {
                const MAXVICINITY: u32 = 4;
                let mut limit = *lenhint(t);
                if limit == 0 {
                    limit = 1;
                }
                let newhint = |h: u32| -> u64 {
                    *lenhint(t) = h;
                    h as u64
                };
                if arr_empty(t, limit as usize - 1) {
                    let mut i = 0;
                    while i < MAXVICINITY && limit > 1 {
                        limit -= 1;
                        if !arr_empty(t, limit as usize - 1) {
                            return newhint(limit);
                        }
                        i += 1;
                    }
                    return newhint(binsearch(t, 0, limit));
                } else {
                    let mut i = 0;
                    while i < MAXVICINITY && limit < asize {
                        limit += 1;
                        if arr_empty(t, limit as usize - 1) {
                            return newhint(limit - 1);
                        }
                        i += 1;
                    }
                    if arr_empty(t, asize as usize - 1) {
                        return newhint(binsearch(t, limit, asize));
                    }
                }
                *lenhint(t) = asize;
            }
            if isdummy(t) || hashkeyisempty(t, asize as u64 + 1) {
                asize as u64
            } else {
                hash_search(heap.seed, t, asize)
            }
        }
    }

    /// `luaH_next`: the entry after `key` (nil = start). `Ok(None)` at the end of the traversal.
    pub fn next(self, key: &Value) -> Result<Option<(Value, Value)>, TableError> {
        let mut hint = 0u32;
        self.next_hinted(key, &mut hint)
    }

    /// `luaH_next` with a carried traversal index (F1): `*hint` is the `findindex` result of the key the previous call
    /// returned (node position + 1 + `asize`, 0 = none). One compare of the node's key with `key` replaces the rehash and
    /// the chain walk of `findindex`; any mismatch (another table, a rehash, a collected key) falls back to the lookup, so the
    /// result is always that of `luaH_next`. `*hint` is updated to the position of the returned hash entry.
    #[inline]
    pub fn next_hinted(
        self,
        key: &Value,
        hint: &mut u32,
    ) -> Result<Option<(Value, Value)>, TableError> {
        let t = self.as_ptr();
        // SAFETY: T1/T2.
        unsafe {
            let asize = (*t).asize;
            let mut i = 'idx: {
                let h = *hint;
                if h > asize && !key.is_nil() && keyinarray(t, key) == 0 {
                    let j = (h - 1 - asize) as usize;
                    if j < sizenode(t) {
                        let n = gnode(t, j);
                        let (kt, kp) = key.to_parts();
                        if (*n).key_tt == kt && (*n).key.addr() == kp.addr() {
                            break 'idx h;
                        }
                    }
                }
                findindex(t, key, asize)?
            };
            while i < asize {
                if !arr_empty(t, i as usize) {
                    return Ok(Some((Value::int(i as i64 + 1), arr_get(t, i as usize))));
                }
                i += 1;
            }
            let mut j = (i - asize) as usize;
            let size = sizenode(t);
            while j < size {
                let n = gnode(t, j);
                if !nval_empty(n) {
                    *hint = (j as u32) + 1 + asize;
                    return Ok(Some((nkey(n), nval(n))));
                }
                j += 1;
            }
            Ok(None)
        }
    }
}

unsafe fn retpsetcode(t: *const Table, slot: *mut Node) -> i32 {
    if slot.is_null() {
        HNOTFOUND
    } else {
        // SAFETY: `slot` is a node of `t`.
        unsafe { slot.offset_from((*t).node) as i32 + HFIRSTNODE }
    }
}

unsafe fn finishnodeset(t: *const Table, slot: *mut Node, val: &Value) -> i32 {
    // SAFETY: slot null or a node of `t`.
    unsafe {
        if !slot.is_null() && !nval_empty(slot) {
            set_nval(slot, val);
            HOK
        } else {
            retpsetcode(t, slot)
        }
    }
}

unsafe fn binsearch(t: *const Table, mut i: u32, mut j: u32) -> u32 {
    // SAFETY: indices within [1, asize].
    unsafe {
        while j - i > 1 {
            let m = (i + j) / 2;
            if arr_empty(t, m as usize - 1) {
                j = m;
            } else {
                i = m;
            }
        }
    }
    i
}

/// 5.4 `alimit`: the array limit (0 without an array part).
#[cfg(rustynail_lua54)]
#[inline]
unsafe fn alimit(t: *const Table) -> u32 {
    // SAFETY: T1 (the hint slot exists when asize > 0).
    unsafe {
        if (*t).asize == 0 {
            0
        } else {
            *lenhint(t)
        }
    }
}

/// 5.4 `ispow2realasize`.
#[cfg(rustynail_lua54)]
unsafe fn ispow2realasize(t: *const Table) -> bool {
    // SAFETY: valid table.
    unsafe { (*t).flags & BITRAS != 0 || (*t).asize.is_power_of_two() }
}

/// 5.4 `setlimittosize`.
#[cfg(rustynail_lua54)]
unsafe fn setlimittosize(t: *mut Table) {
    // SAFETY: T1.
    unsafe {
        if (*t).asize > 0 {
            *lenhint(t) = (*t).asize;
        }
        (*t).flags &= !BITRAS;
    }
}

/// 5.4 `hash_search` (no randomized probing).
#[cfg(rustynail_lua54)]
unsafe fn hash_search54(t: *const Table, mut j: u64) -> u64 {
    const MAXI: u64 = i64::MAX as u64;
    let mut i;
    if j == 0 {
        j += 1; // the caller ensures 'j + 1' is present
    }
    // SAFETY: valid table.
    unsafe {
        loop {
            i = j; // 'i' is a present index
            if j <= MAXI / 2 {
                j *= 2;
            } else {
                j = MAXI;
                if hashkeyisempty(t, j) {
                    break; // 'j' now is an absent index
                } else {
                    return j; // max integer is a boundary
                }
            }
            if hashkeyisempty(t, j) {
                break;
            }
        }
        while j - i > 1 {
            let m = (i + j) / 2;
            if hashkeyisempty(t, m) {
                j = m;
            } else {
                i = m;
            }
        }
    }
    i
}

#[cfg_attr(rustynail_lua54, allow(dead_code))]
unsafe fn hash_search(seed: u32, t: *const Table, asize: u32) -> u64 {
    const MAXI: u64 = i64::MAX as u64;
    let mut i: u64 = asize as u64 + 1;
    let mut rnd = seed;
    let n = if asize > 0 { ceillog2(asize) } else { 0 };
    let mask: u32 = if n >= 32 { u32::MAX } else { (1u32 << n) - 1 };
    let incr = (rnd & mask) as u64 + 1;
    let mut j: u64 = if incr <= MAXI - i { i + incr } else { i + 1 };
    rnd = if n >= 32 { 0 } else { rnd >> n };
    // SAFETY: valid table.
    unsafe {
        while !hashkeyisempty(t, j) {
            i = j;
            if j < MAXI / 2 {
                j = j * 2 + (rnd & 1) as u64;
                rnd >>= 1;
            } else {
                j = MAXI;
                if hashkeyisempty(t, j) {
                    break;
                } else {
                    return j;
                }
            }
        }
        while j - i > 1 {
            let m = (i + j) / 2;
            if hashkeyisempty(t, m) {
                j = m;
            } else {
                i = m;
            }
        }
    }
    i
}

// ---------------------------------------------------------------------------------------------
// helpers for the collector (same module: they touch private layout)
// ---------------------------------------------------------------------------------------------

/// Layout-level access used by `gc`: iterate hash nodes and array slots.
pub(crate) mod gcaccess {
    use super::*;

    pub unsafe fn asize(t: *const Table) -> u32 {
        // SAFETY: caller passes a valid table.
        unsafe { (*t).asize }
    }
    pub unsafe fn nodes(t: *const Table) -> usize {
        // SAFETY: caller passes a valid table.
        unsafe { sizenode(t) }
    }
    /// 5.4 `traversetable` work: `1 + alimit + 2 * allocsizenode(h)`.
    #[cfg(rustynail_lua54)]
    pub unsafe fn work54(t: *const Table) -> usize {
        // SAFETY: caller passes a valid table; the hint slot exists when `BITRAS` is set (T1).
        unsafe {
            let alimit = if (*t).flags & BITRAS != 0 {
                *lenhint(t) as usize
            } else {
                (*t).asize as usize
            };
            let nodes = if isdummy(t) { 0 } else { sizenode(t) };
            1 + alimit + 2 * nodes
        }
    }
    pub unsafe fn node(t: *const Table, i: usize) -> *mut Node {
        // SAFETY: caller keeps i < nodes(t).
        unsafe { gnode(t, i) }
    }
    pub unsafe fn node_empty(n: *const Node) -> bool {
        // SAFETY: valid node.
        unsafe { nval_empty(n) }
    }
    /// Collectable object in the value of node `n`, or null.
    pub unsafe fn node_val_gc(n: *const Node) -> *mut GcHeader {
        // SAFETY: valid node.
        unsafe {
            if (*n).val_tt & BIT_ISCOLLECTABLE != 0 {
                (*n).val.gc()
            } else {
                std::ptr::null_mut()
            }
        }
    }
    /// Collectable object in the key of node `n`, or null (dead keys are not collectable).
    pub unsafe fn node_key_gc(n: *const Node) -> *mut GcHeader {
        // SAFETY: valid node.
        unsafe {
            if key_is_collectable(n) {
                (*n).key.gc()
            } else {
                std::ptr::null_mut()
            }
        }
    }
    pub unsafe fn set_node_empty(n: *mut Node) {
        // SAFETY: valid node.
        unsafe { (*n).val_tt = LUA_VEMPTY }
    }
    /// `clearkey`: a node with an empty value gets a dead key.
    pub unsafe fn clearkey(n: *mut Node) {
        // SAFETY: valid node with empty value.
        unsafe {
            if key_is_collectable(n) {
                (*n).key_tt = LUA_TDEADKEY;
            }
        }
    }
    pub unsafe fn arr_gc(t: *const Table, i: usize) -> *mut GcHeader {
        // SAFETY: i < asize.
        unsafe {
            if *arr_tag(t, i) & BIT_ISCOLLECTABLE != 0 {
                (*arr_val(t, i)).gc()
            } else {
                std::ptr::null_mut()
            }
        }
    }
    pub unsafe fn set_arr_empty(t: *const Table, i: usize) {
        // SAFETY: i < asize.
        unsafe { *arr_tag(t, i) = LUA_VEMPTY }
    }
}

const _: () = assert!(std::mem::size_of::<Table>() == 48);
const _: () = assert!(std::mem::size_of::<GcHeader>() == 16);
#[cfg(test)]
mod fastmod_tests {
    use super::fastmod;

    /// `fastmod(n, l) == n % ((2^l - 1) | 1)` for every node-size exponent a table can have: edge values around
    /// each modulus and its multiples, plus random inputs (xorshift, fixed seed).
    #[test]
    fn fastmod_exact() {
        let mut x: u64 = 0x9E3779B97F4A7C15;
        for l in 0..32u8 {
            let d = (((1u64 << l) - 1) | 1) as u32;
            let mut edges = vec![
                0u32,
                1,
                2,
                3,
                u32::MAX,
                u32::MAX - 1,
                i32::MAX as u32,
                i32::MAX as u32 + 1,
            ];
            for m in [1u64, 2, 3, 7, 1000, 65535, 65536] {
                let b = (d as u64).wrapping_mul(m);
                for off in [-1i64, 0, 1] {
                    let v = b as i64 + off;
                    if (0..=u32::MAX as i64).contains(&v) {
                        edges.push(v as u32);
                    }
                }
            }
            for n in edges {
                assert_eq!(fastmod(n, l), n % d, "n={n} l={l}");
            }
            for _ in 0..100_000 {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let n = x as u32;
                assert_eq!(fastmod(n, l), n % d, "n={n} l={l}");
                let small = (x >> 40) as u32 % d.saturating_mul(3);
                assert_eq!(fastmod(small, l), small % d, "n={small} l={l}");
            }
        }
    }

    /// Every residue of small moduli, exhaustively.
    #[test]
    fn fastmod_exhaustive_small() {
        for l in 0..12u8 {
            let d = (((1u64 << l) - 1) | 1) as u32;
            for n in 0..(d * 5 + 17) {
                assert_eq!(fastmod(n, l), n % d);
            }
        }
    }
}
