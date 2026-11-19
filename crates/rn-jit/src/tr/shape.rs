//! S4: layout ids of hash parts and the interning behind `HSHAPE`.
//!
//! A hash part with at least 8 nodes carries a 64-bit *layout id* slot in front of its node vector (`rn_core::table`, `layout_id`): 0 = not
//! known, any other value = "the node vector's key layout is exactly the interned descriptor with that id". The descriptor is the content of
//! the vector (`TableRef::layout_descriptor`: `lsizenode` and, per node with a key, its index, key tag and key bits), so **equal layouts get
//! equal ids across tables** (hidden-class style: the five `bodies[i]` of nbody share one id) and an id says nothing but bits: it never
//! names an object.
//!
//! The core clears the slot (stores 0) at every operation that changes which key lives in which node (insertion including the
//! collide-move, a rehash allocates a fresh vector, GC clearing of dead keys and weak-table clears, the trace's inline `NEWREF`), so a table
//! whose slot is `id` has had that layout since the id was stored. The key *pointers* in a descriptor are therefore never stale in a way
//! that matters: a table with slot `id` has, in node `i`, a key whose bits are the descriptor's; if the string that lived at that address was
//! freed and another string allocated there, the table that holds the new one has the same bits and the same id, and the trace's guard
//! asserts exactly bit equality at the node, like the key compare of `HREFK` it replaces (see docs/JIT_TRACE.md, "Shape guards").
//!
//! Ids are handed out monotonically from 1 and never reused. They stay below `2^31` (the guard compares against a sign-extended 32-bit
//! immediate); when the counter or the table of descriptors is full no new id is made (`layout_id` answers 0 and the recorder emits no
//! shape guard for such a table).

use crate::fx::HashMap;
use rn_core::table::{self, Node, TableRef};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Smallest `lsizenode` that has a layout-id slot.
pub const MIN_LSIZE: u8 = 3;
/// Largest `lsizenode` that gets an id (a descriptor is built by scanning every node).
pub const MAX_LSIZE: u8 = 6;
/// Most distinct descriptors interned (the table of descriptors never shrinks).
pub const MAX_DESCS: usize = 4096;
/// The first id that is never handed out.
pub const ID_LIMIT: u64 = (1 << 31) - 1;

struct Interner {
    map: HashMap<Vec<u64>, u64>,
    next: u64,
}

static INTERN: Mutex<Option<Interner>> = Mutex::new(None);

/// Ids made (statistics for tests and `RUSTYNAIL_JIT_VERBOSE`).
pub static IDS_MADE: AtomicU64 = AtomicU64::new(0);
/// Zero ids filled by the exit handler.
pub static FILLS: AtomicU64 = AtomicU64::new(0);
/// Shape-guard exits taken (the handler ran; zero ids filled and other layouts alike).
pub static MISSES: AtomicU64 = AtomicU64::new(0);
/// Families re-recorded without shape guards (`cache::shape_miss`).
pub static DEMOTED: AtomicU64 = AtomicU64::new(0);

/// The id of the descriptor `d`: the interned one, or a new one; 0 when no id can be made.
fn intern(d: Vec<u64>) -> u64 {
    let mut g = INTERN.lock().unwrap_or_else(|e| e.into_inner());
    let it = g.get_or_insert_with(|| Interner {
        map: HashMap::default(),
        next: 1,
    });
    if let Some(&id) = it.map.get(&d) {
        return id;
    }
    // MUTANT `shape_id_reuse`: the counter is reset, so a new layout may get the id of another one
    if crate::tr::mutants::active("shape_id_reuse") {
        it.next = 1 + (it.map.len() as u64 & 1);
    } else if it.map.len() >= MAX_DESCS || it.next >= ID_LIMIT {
        return 0;
    }
    let id = it.next;
    it.next += 1;
    it.map.insert(d, id);
    IDS_MADE.fetch_add(1, Ordering::Relaxed);
    id
}

/// Can a table with this `lsizenode` carry an id?
pub const fn eligible(lsizenode: u8) -> bool {
    lsizenode >= MIN_LSIZE && lsizenode <= MAX_LSIZE
}

/// The layout id of `t`'s node vector, computed and stored when the slot is 0; 0 when the table has no slot, is too large, or no id could be made.
pub fn layout_id(t: TableRef) -> u64 {
    if !eligible(t.lsizenode()) {
        return 0;
    }
    let cur = t.layout_id();
    if cur != 0 {
        return cur;
    }
    let mut d = Vec::new();
    t.layout_descriptor(&mut d);
    let id = intern(d);
    if id != 0 {
        t.set_layout_id(id);
    }
    id
}

/// [`layout_id`] of a bare node vector (the exit handler of a shape guard knows the vector and its size): fills the slot when it is 0.
/// Returns the slot's value afterwards.
///
/// # Safety
/// `node` is the first node of a live hash part of `2^lsizenode` nodes.
pub unsafe fn fill_node(node: *mut Node, lsizenode: u8) -> u64 {
    if !eligible(lsizenode) {
        return 0;
    }
    // SAFETY: contract of this function.
    let Some(slot) = (unsafe { table::layout_slot_of(node, lsizenode) }) else {
        return 0;
    };
    // SAFETY: the slot is inside the part's allocation (T2).
    let cur = unsafe { slot.read() };
    if cur != 0 {
        return cur;
    }
    let mut d = Vec::new();
    // SAFETY: contract of this function.
    unsafe { table::layout_descriptor_raw(node, lsizenode, &mut d) };
    let id = intern(d);
    if id != 0 {
        // SAFETY: as the read above.
        unsafe { slot.write(id) };
        FILLS.fetch_add(1, Ordering::Relaxed);
    }
    id
}

/// The slot of a bare node vector (0 when none).
///
/// # Safety
/// As [`fill_node`].
pub unsafe fn slot_of(node: *mut Node, lsizenode: u8) -> u64 {
    // SAFETY: contract of this function.
    match unsafe { table::layout_slot_of(node, lsizenode) } {
        // SAFETY: the slot is inside the part's allocation (T2).
        Some(s) => unsafe { s.read() },
        None => 0,
    }
}
