//! Per-state data of the facade, kept *outside* the Lua state so that no Rust object is ever
//! stored in (or read back from) raw userdata memory.
//!
//! Rust values owned by Lua (callbacks, typed userdata) live in [`Shared::objects`], keyed by a
//! 64-bit id that never repeats within a state. A Lua object refers to its Rust value only through
//! that id, stored as an *integer user value* (or closure upvalue), which safe code can read. A
//! lookup by id is followed by a checked downcast, so a forged or stale id (for example one
//! installed by `debug.setuservalue`) can only produce an [`ErrorKind::Api`] error or the wrong
//! object of the right type, never undefined behaviour.
//!
//! The table is keyed by the address of the state's `GlobalState`, which is stable for the life
//! of the state; [`crate::Lua`] registers it on creation and removes it after closing the state.
//! States are not `Send`, so every callback runs on the thread that created the state and the
//! thread-local table is the right place.
//!
//! [`ErrorKind::Api`]: crate::ErrorKind::Api
use rn_core::state::{GlobalState, State};
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub(crate) struct Shared {
    /// Rust values owned by Lua objects.
    pub objects: HashMap<i64, Rc<dyn Any>>,
    pub next_id: i64,
    /// Sequence number of the error object stored in the registry slot `err_slot`.
    pub err_seq: u64,
    /// Registry integer key holding the last caught error object.
    pub err_slot: i64,
    /// Registry reference of the metatable of the closure-owner userdata (`__gc` drops the
    /// Rust closure).
    pub callback_mt: i32,
    /// Registry references of the metatables of typed userdata.
    pub type_mts: HashMap<TypeId, i32>,
    /// The instruction-limit hook fired and its error is still unwinding.
    pub limit_hit: bool,
    /// Instruction budget per [`crate::Lua::set_instruction_limit`] (0: none).
    pub limit: u32,
    /// Nesting depth of Rust callbacks currently running.
    pub callback_depth: u32,
}

thread_local! {
    static STATES: RefCell<HashMap<usize, Shared>> = RefCell::new(HashMap::new());
}

fn key(st: &State) -> usize {
    std::ptr::from_ref::<GlobalState>(st.g()).addr()
}

pub(crate) fn register(st: &State, err_slot: i64) {
    let sh = Shared {
        objects: HashMap::new(),
        next_id: 1,
        err_seq: 0,
        err_slot,
        callback_mt: 0,
        type_mts: HashMap::new(),
        limit_hit: false,
        limit: 0,
        callback_depth: 0,
    };
    let _ = STATES.try_with(|m| m.borrow_mut().insert(key(st), sh));
}

/// Remove the state's entry; the Rust objects are dropped after the table borrow is released.
pub(crate) fn unregister(k: usize) {
    let sh = STATES
        .try_with(|m| m.borrow_mut().remove(&k))
        .ok()
        .flatten();
    drop(sh);
}

pub(crate) fn state_key(st: &State) -> usize {
    key(st)
}

/// Run `f` on the state's data (`None` if the state is not a facade state, or during thread
/// teardown). `f` must not call back into Lua.
pub(crate) fn with<R>(st: &State, f: impl FnOnce(&mut Shared) -> R) -> Option<R> {
    STATES
        .try_with(|m| m.borrow_mut().get_mut(&key(st)).map(f))
        .ok()
        .flatten()
}

/// Store a Rust object; returns its id.
pub(crate) fn insert(st: &State, obj: Rc<dyn Any>) -> Option<i64> {
    with(st, |sh| {
        let id = sh.next_id;
        sh.next_id += 1;
        sh.objects.insert(id, obj);
        id
    })
}

/// The object with id `id` (a clone of the `Rc`, so it stays alive while in use even if Lua
/// finalizes its owner meanwhile).
pub(crate) fn get(st: &State, id: i64) -> Option<Rc<dyn Any>> {
    with(st, |sh| sh.objects.get(&id).cloned()).flatten()
}

/// Remove an object (finalizer). The value is returned so the caller drops it outside the borrow.
pub(crate) fn remove(st: &State, id: i64) -> Option<Rc<dyn Any>> {
    with(st, |sh| sh.objects.remove(&id)).flatten()
}
