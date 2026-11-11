//! F3: the class-A helpers of TR1 (`TNEW`, `NEWREF`, `TLEN` of `tr::asm::jit_tr_alloc`) in isolation, on a real state.
//!
//! The contract the traces rely on (docs/JIT_TRACE.md 6.9): a helper that reports 0 has **done nothing** (no allocation,
//! no insertion, the GC accounting unchanged) and does so exactly when the interpreter's `check_gc` right after the same
//! allocation would step the collector (or the allocation could fail); otherwise it allocates with the interpreter's own
//! accounting. Detectors of the mutants `tnew_no_gc_check`, `tnew_bytes_small`, `newref_present_ok`, `newref_fail_ignored`.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, State};
use rn_jit::tr::asm::{newref_core, tnew_core};

fn state() -> LuaState {
    let mut st = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, rn_jit::Mode::Off);
    st.heap().gc_set_stp(0);
    st
}

/// `NEWTABLE B C` as the interpreter does it (no GC step): the bytes it adds to the accounting.
fn interpreter_bytes(st: &mut State, b: u32, c: u32) -> usize {
    let before = st.heap().total_bytes();
    let heap = st.heap();
    let t = heap.new_table().expect("table");
    let hsize = if b > 0 { 1u32 << (b - 1) } else { 0 };
    if b != 0 || c != 0 {
        t.resize(heap, c, hsize).expect("resize");
    }
    st.heap().total_bytes() - before
}

const SHAPES: &[(u32, u32)] = &[
    (0, 0),
    (0, 1),
    (0, 3),
    (0, 100),
    (1, 0),
    (2, 5),
    (3, 0),
    (4, 17),
    (7, 1000),
    (12, 4096),
];

/// `new_table_bytes` is exactly what `Heap::new_table` + `resize` count.
#[test]
fn new_table_bytes_is_what_newtable_allocates() {
    let mut st = state();
    for &(b, c) in SHAPES {
        let hsize = if b > 0 { 1u32 << (b - 1) } else { 0 };
        let got = interpreter_bytes(&mut st, b, c);
        assert_eq!(
            rn_core::table::new_table_bytes(c, hsize),
            Some(got),
            "NEWTABLE B={b} C={c}"
        );
    }
    assert_eq!(rn_core::table::new_table_bytes(u32::MAX, 0), None);
    assert_eq!(rn_core::table::new_table_bytes(0, u32::MAX), None);
}

/// The helper allocates iff the collector would stay idle after the allocation: the threshold is crossed by exactly the
/// bytes the allocation adds (`total + bytes >= threshold` = due, as `gc_due` after the allocation).
#[test]
fn tnew_exits_exactly_when_a_step_would_be_due() {
    let mut st = state();
    for &(b, c) in SHAPES {
        let hsize = if b > 0 { 1u32 << (b - 1) } else { 0 };
        let bytes = rn_core::table::new_table_bytes(c, hsize).unwrap();
        let total = st.heap().total_bytes();
        // due after the allocation (the interpreter would step): nothing done
        for thr in [total + bytes - 1, total + bytes, total, 0] {
            st.heap().gc_set_threshold(thr);
            assert_eq!(
                tnew_core(&mut st, b, c),
                0,
                "B={b} C={c} threshold {thr}: must exit"
            );
            assert_eq!(
                st.heap().total_bytes(),
                total,
                "B={b} C={c}: the exit allocated"
            );
        }
        // idle after the allocation (threshold one byte beyond): allocated, with the interpreter's accounting
        st.heap().gc_set_threshold(total + bytes + 1);
        let t = tnew_core(&mut st, b, c);
        assert_ne!(t, 0, "B={b} C={c}: must allocate");
        assert_eq!(
            st.heap().total_bytes(),
            total + bytes,
            "B={b} C={c}: accounting"
        );
        // the table has the interpreter's shape
        // SAFETY: the helper returned a live table of this state.
        let t = unsafe { rn_core::table::TableRef::from_raw(t as *mut rn_core::table::Table) };
        assert_eq!(t.array_size(), c);
        assert!(
            !st.heap().gc_due(),
            "B={b} C={c}: the collector must stay idle"
        );
        st.heap().gc_set_threshold(usize::MAX);
    }
}

/// A stopped collector never steps: the helper allocates whatever the threshold says. The stress modes always step.
#[test]
fn tnew_follows_stop_and_stress() {
    let mut st = state();
    st.heap().gc_set_threshold(0);
    st.heap().gc_set_stp(rn_core::gc::GCSTP_USER);
    assert_ne!(tnew_core(&mut st, 0, 0), 0, "stopped: allocates");
    st.heap().gc_set_stp(0);
    assert_eq!(tnew_core(&mut st, 0, 0), 0, "threshold crossed: exits");
}

/// The memory limit: an allocation that would fail is not attempted.
#[test]
fn tnew_exits_when_the_limit_would_fail() {
    let mut st = state();
    let (b, c) = (2, 10);
    let bytes = rn_core::table::new_table_bytes(c, 2).unwrap();
    let total = st.heap().total_bytes();
    st.heap().gc_set_threshold(usize::MAX);
    st.heap().mem_mut().set_limit(total + bytes - 1);
    assert_eq!(tnew_core(&mut st, b, c), 0);
    assert_eq!(st.heap().total_bytes(), total);
    st.heap().mem_mut().set_limit(total + bytes);
    assert_ne!(tnew_core(&mut st, b, c), 0);
    st.heap().mem_mut().set_limit(usize::MAX);
}

/// P-J: `resize` of a fresh table (the no-temporary-table path) that fails on the ARRAY allocation after the node vector was
/// allocated gives the node vector back: the accounting is the empty table's and a retry with room gives the exact bytes.
#[test]
fn fresh_table_resize_failure_is_atomic() {
    let mut st = state();
    let (hsize, c) = (4u32, 10u32);
    let all = rn_core::table::new_table_bytes(c, hsize).unwrap();
    let empty = rn_core::table::new_table_bytes(0, 0).unwrap();
    let total = st.heap().total_bytes();
    st.heap().gc_set_threshold(usize::MAX);
    // room for the table and the node vector, one byte short of the array
    st.heap().mem_mut().set_limit(total + all - 1);
    let heap = st.heap();
    let t = heap.new_table().expect("table");
    assert!(t.resize(heap, c, hsize).is_err());
    assert_eq!(
        st.heap().total_bytes(),
        total + empty,
        "the node vector was not given back"
    );
    assert_eq!(t.array_size(), 0);
    st.heap().mem_mut().set_limit(total + all);
    let heap = st.heap();
    t.resize(heap, c, hsize)
        .expect("room for exactly this shape");
    assert_eq!(st.heap().total_bytes(), total + all);
    assert_eq!((t.array_size(), t.hash_size()), (c, hsize as usize));
    st.heap().mem_mut().set_limit(usize::MAX);
}

fn key(st: &mut State, s: &str) -> (rn_core::string::StrRef, u64) {
    let k = st.new_str(s.as_bytes()).expect("string");
    (k, k.as_ptr() as u64)
}

/// `NEWREF` inserts an absent key (also through a rehash) and returns its node; a present key does nothing.
#[test]
fn newref_inserts_absent_keys_only() {
    let mut st = state();
    st.heap().gc_set_threshold(usize::MAX);
    let t = st.heap().new_table().unwrap();
    let tp = t.as_ptr() as u64;
    let mut keys = Vec::new();
    for i in 0..40 {
        let (k, kp) = key(&mut st, &format!("key{i}"));
        // SAFETY: a live table and a live interned short string.
        let node = unsafe { newref_core(&mut st, tp, kp) };
        assert_ne!(node, 0, "key{i}: absent key must be inserted");
        let ix = t.short_str_node(k).expect("key present");
        assert_eq!(node, t.node_addr(ix) as u64, "key{i}: node address");
        // the placeholder is replaced by the trace's HSTORE; here the key holds `true`
        keys.push((k, kp));
        // a second insertion of the same key does nothing
        let before = (t.hash_size(), t.array_size());
        // SAFETY: as above.
        assert_eq!(
            unsafe { newref_core(&mut st, tp, kp) },
            0,
            "key{i}: present key"
        );
        assert_eq!((t.hash_size(), t.array_size()), before);
    }
    for (k, _) in &keys {
        assert!(t.short_str_node(*k).is_some());
    }
}

/// A key that is present with a nil value is "present": the interpreter reuses the node, the helper leaves it alone.
#[test]
fn newref_leaves_nil_valued_keys_to_the_interpreter() {
    let mut st = state();
    st.heap().gc_set_threshold(usize::MAX);
    let t = st.heap().new_table().unwrap();
    let (k, kp) = key(&mut st, "x");
    t.set_str(st.heap(), k, &rn_core::value::Value::int(1))
        .unwrap();
    t.set_str(st.heap(), k, &rn_core::value::Value::NIL)
        .unwrap();
    // SAFETY: a live table and a live interned short string.
    assert_eq!(unsafe { newref_core(&mut st, t.as_ptr() as u64, kp) }, 0);
}

/// An insertion that needs memory the limit refuses reports 0 and leaves the table as it was (`luaH_resize` frees what
/// it allocated).
#[test]
fn newref_failure_leaves_the_table_intact() {
    let mut st = state();
    st.heap().gc_set_threshold(usize::MAX);
    let t = st.heap().new_table().unwrap();
    for i in 0..3 {
        let (k, kp) = key(&mut st, &format!("k{i}"));
        // SAFETY: a live table and a live interned short string.
        assert_ne!(unsafe { newref_core(&mut st, t.as_ptr() as u64, kp) }, 0);
        let _ = k;
    }
    let (k, kp) = key(&mut st, "overflow");
    let (total, hs) = (st.heap().total_bytes(), t.hash_size());
    // the table has 4 nodes and 3 keys; fill it, then the next insertion must rehash
    let (k4, k4p) = key(&mut st, "fourth");
    // SAFETY: as above.
    assert_ne!(unsafe { newref_core(&mut st, t.as_ptr() as u64, k4p) }, 0);
    let total = st.heap().total_bytes().max(total);
    let hs = t.hash_size().max(hs);
    st.heap().mem_mut().set_limit(total + 8);
    // SAFETY: as above.
    assert_eq!(
        unsafe { newref_core(&mut st, t.as_ptr() as u64, kp) },
        0,
        "no memory for the rehash"
    );
    assert_eq!(
        st.heap().total_bytes(),
        total,
        "the failed insertion left bytes behind"
    );
    assert_eq!(t.hash_size(), hs);
    assert!(t.short_str_node(k).is_none());
    assert!(t.short_str_node(k4).is_some());
    st.heap().mem_mut().set_limit(usize::MAX);
}
