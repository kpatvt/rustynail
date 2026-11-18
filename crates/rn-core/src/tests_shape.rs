//! S4: the per-hash-part layout id (the 64-bit slot at `node - 16` of hash parts with at least 8 nodes).
//!
//! These tests are the soundness spec of the slot: a later trace guard treats "the id still has the value I stored" as "no key
//! of this hash part changed its node". The tests stamp a nonzero marker with `set_layout_id` and check that every operation that
//! changes which key lives in which node (insert, collision move, rehash/resize, GC dead-key marking, weak-table clears) leaves
//! 0 behind, and that operations that do not (value writes of live keys, nil stores, array writes, reads) keep the marker.
use crate::gc::{GcKind, Marker};
use crate::heap::Heap;
use crate::table::TableRef;
use crate::tests_table::Rng;
use crate::value::Value;

const MARK: u64 = 0x5EED_0000_0000_0001;

fn s(h: &mut Heap, text: &str) -> Value {
    Value::string(h.new_str(text.as_bytes()).unwrap())
}

fn collect_with(h: &mut Heap, roots: &[Value]) {
    h.collect(&mut |m: &mut Marker| {
        for v in roots {
            m.mark_value(v);
        }
    });
    h.verify();
}

/// A table with a hash part of exactly `n` nodes (a power of two >= 1) and `k` string keys `prefix0..` set to 1.
fn table_with(h: &mut Heap, n: u32, k: u32, prefix: &str) -> (TableRef, Vec<Value>) {
    let t = h.new_table().unwrap();
    t.resize(h, 0, n).unwrap();
    let mut keys = Vec::new();
    for i in 0..k {
        let key = s(h, &format!("{prefix}{i}"));
        t.set(h, &key, &Value::int(1)).unwrap();
        keys.push(key);
    }
    (t, keys)
}

fn stamp(t: TableRef) {
    assert!(t.set_layout_id(MARK));
    assert_eq!(t.layout_id(), MARK);
}

/// (node count, per node (key tag, key bits)): what the layout id stands for.
fn layout(t: TableRef) -> (usize, Vec<(u8, u64)>) {
    let n = t.hash_size();
    (n, (0..n).map(|i| t.node_key(i)).collect())
}

#[test]
fn slot_exists_from_eight_nodes_and_is_born_zero() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    assert_eq!(t.layout_id(), 0, "dummy node: no slot, reads 0");
    assert!(!t.set_layout_id(MARK), "dummy node has no slot");
    assert_eq!(t.layout_id(), 0);
    for n in [1u32, 2, 4] {
        t.resize(&mut h, 0, n).unwrap();
        assert!(!t.set_layout_id(MARK), "{n} nodes: no slot");
        assert_eq!(t.layout_id(), 0);
    }
    for n in [8u32, 16, 32, 64, 1024] {
        t.resize(&mut h, 0, n).unwrap();
        assert_eq!(t.hash_size(), n as usize);
        assert_eq!(t.layout_id(), 0, "{n} nodes: born 0");
        stamp(t);
        // the neighbouring slot (`lastfree`) is untouched by the stamp: inserts still work and find free nodes
        for i in 0..n {
            let key = s(&mut h, &format!("k{i}"));
            t.set(&mut h, &key, &Value::int(i as i64)).unwrap();
        }
        assert_eq!(t.hash_size(), n as usize, "filled without a rehash");
        assert_eq!(t.layout_id(), 0);
        for i in 0..n {
            let key = s(&mut h, &format!("k{i}"));
            assert_eq!(t.get(&key).as_int(), Some(i as i64));
        }
    }
    // a hash part allocated by `Heap::new_table` + `resize` after a stamped one is born 0 again
    let u = h.new_table().unwrap();
    u.resize(&mut h, 0, 8).unwrap();
    assert_eq!(u.layout_id(), 0);
}

#[test]
fn new_key_without_rehash_clears() {
    let mut h = Heap::new(0).unwrap();
    for n in [8u32, 16, 32] {
        // every insert (free main position or collision) clears; at least one of them collides
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, n).unwrap();
        let mut collided = false;
        for i in 0..n {
            stamp(t);
            let key = s(&mut h, &format!("n{n}_{i}"));
            t.set(&mut h, &key, &Value::int(1)).unwrap();
            assert_eq!(t.layout_id(), 0, "insert #{i} into {n} nodes");
            assert_eq!(t.hash_size(), n as usize);
            collided |= (0..n as usize).any(|j| t.node_next(j) != 0);
        }
        assert!(collided, "{n} keys in {n} nodes must collide");
        // integer and float keys use the same insert
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, 8).unwrap();
        stamp(t);
        t.set(&mut h, &Value::int(1000), &Value::int(1)).unwrap();
        assert_eq!(t.layout_id(), 0);
        stamp(t);
        t.set(&mut h, &Value::float(1.5), &Value::int(1)).unwrap();
        assert_eq!(t.layout_id(), 0);
        stamp(t);
        t.set(&mut h, &Value::TRUE, &Value::int(1)).unwrap();
        assert_eq!(t.layout_id(), 0);
        stamp(t);
        t.set_int(&mut h, 77, &Value::int(1)).unwrap();
        assert_eq!(t.layout_id(), 0);
    }
}

#[test]
fn collision_that_moves_a_live_key_clears() {
    // find a sequence of inserts where an insert moves a live key to the free node (the colliding node was out of its main
    // position); the id must be 0 after it
    let mut moved = 0;
    for seed in 1..200u64 {
        let mut h = Heap::new(seed as u32).unwrap();
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, 8).unwrap();
        let mut r = Rng(seed);
        for _ in 0..8 {
            let before = layout(t);
            stamp(t);
            let key = Value::int(r.below(1000) as i64 + 100);
            t.set(&mut h, &key, &Value::int(1)).unwrap();
            if t.hash_size() != 8 {
                break; // rehash: the new vector is born 0
            }
            let after = layout(t);
            // a node that held a live key before and holds another one now: a key moved (or was overwritten)
            let moved_key = (0..8).any(|i| {
                before.1[i].0 != 0
                    && before.1[i].0 != crate::value::LUA_TDEADKEY
                    && before.1[i] != after.1[i]
            });
            if moved_key {
                moved += 1;
            }
            if before != after {
                assert_eq!(t.layout_id(), 0, "seed {seed}: layout changed, id kept");
            }
        }
    }
    assert!(moved > 0, "no collision move was exercised");
}

#[test]
fn rehash_and_resize_give_a_fresh_zero_id() {
    let mut h = Heap::new(0).unwrap();
    let (t, _) = table_with(&mut h, 8, 8, "r");
    stamp(t);
    // the ninth key does not fit: rehash to 16 nodes
    let key = s(&mut h, "overflow");
    t.set(&mut h, &key, &Value::int(1)).unwrap();
    assert_eq!(t.hash_size(), 16);
    assert_eq!(t.layout_id(), 0);
    stamp(t);
    // explicit resizes (grow, same size, shrink to a part with a slot, shrink to none)
    t.resize(&mut h, 0, 16).unwrap();
    assert_eq!(
        t.layout_id(),
        0,
        "resize to the same size rebuilds the part"
    );
    stamp(t);
    t.resize(&mut h, 4, 32).unwrap();
    assert_eq!(t.layout_id(), 0);
    stamp(t);
    t.set(&mut h, &key, &Value::NIL).unwrap();
    for i in 0..4 {
        let k = s(&mut h, &format!("r{i}"));
        t.set(&mut h, &k, &Value::NIL).unwrap();
    }
    t.resize(&mut h, 4, 8).unwrap();
    assert_eq!(t.layout_id(), 0);
    stamp(t);
    for i in 0..8 {
        let k = s(&mut h, &format!("r{i}"));
        t.set(&mut h, &k, &Value::NIL).unwrap();
    }
    t.resize(&mut h, 0, 0).unwrap();
    assert_eq!(t.layout_id(), 0, "no hash part: reads 0");
    // array-size change only keeps the part's size but rebuilds it too
    t.resize(&mut h, 0, 8).unwrap();
    stamp(t);
    t.resize_array(&mut h, 3).unwrap();
    assert_eq!(t.layout_id(), 0);
    // integer keys migrating between the array and the hash part (rehash with computesizes)
    let t = h.new_table().unwrap();
    t.resize(&mut h, 0, 8).unwrap();
    for i in 1..=8 {
        t.set_int(&mut h, i * 3, &Value::int(1)).unwrap();
    }
    stamp(t);
    for i in 1..=40 {
        t.set_int(&mut h, i, &Value::int(1)).unwrap();
    }
    assert_eq!(t.layout_id(), 0, "grown through rehashes");
}

#[test]
fn value_writes_array_writes_and_reads_keep_the_id() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    t.resize(&mut h, 8, 16).unwrap();
    let mut keys = Vec::new();
    for i in 0..10 {
        let key = s(&mut h, &format!("f{i}"));
        t.set(&mut h, &key, &Value::int(i)).unwrap();
        keys.push(key);
    }
    for i in 1..=8 {
        t.set_int(&mut h, i, &Value::int(i)).unwrap();
    }
    let before = layout(t);
    stamp(t);
    // writes to existing live keys, of every value type
    for (i, key) in keys.iter().enumerate() {
        t.set(&mut h, key, &Value::int(100 + i as i64)).unwrap();
        t.set(&mut h, key, &Value::float(0.5)).unwrap();
        let tv = Value::table(t);
        t.set(&mut h, key, &tv).unwrap();
        let sv = s(&mut h, "value");
        t.set(&mut h, key, &sv).unwrap();
        assert_eq!(t.layout_id(), MARK, "value write of {i}");
    }
    // nil store, then the same key set to non-nil again while still live
    for key in &keys {
        t.set(&mut h, key, &Value::NIL).unwrap();
        assert_eq!(t.layout_id(), MARK, "nil store");
        assert!(t.get(key).is_nil());
    }
    // array part writes and nil stores
    for i in 1..=8 {
        t.set_int(&mut h, i, &Value::int(-i)).unwrap();
        t.set_int(&mut h, i, &Value::NIL).unwrap();
        t.set_int(&mut h, i, &Value::TRUE).unwrap();
    }
    assert_eq!(t.layout_id(), MARK, "array writes");
    // reads
    for key in &keys {
        let _ = t.get(key);
    }
    let _ = t.get_int(3);
    let _ = t.len(&h);
    let mut k = Value::NIL;
    while let Some((nk, _)) = t.next(&k).unwrap() {
        k = nk;
    }
    assert_eq!(t.layout_id(), MARK, "reads");
    assert_eq!(layout(t).1.len(), before.1.len());
    assert_eq!(layout(t), before, "no key moved");
}

#[test]
fn nil_valued_key_revived_in_place_is_a_valid_either_way() {
    // setting a nil-valued (not yet collected) key back to non-nil may be an in-place store or a re-insert; either is sound
    // as long as the node's key is the same afterwards. This pins down which one the implementation does, so a change is noticed.
    let mut h = Heap::new(0).unwrap();
    let (t, keys) = table_with(&mut h, 8, 5, "v");
    stamp(t);
    let before = layout(t);
    t.set(&mut h, &keys[2], &Value::NIL).unwrap();
    t.set(&mut h, &keys[2], &Value::int(9)).unwrap();
    assert_eq!(layout(t), before);
    assert_eq!(
        t.layout_id(),
        MARK,
        "revival of a live nil-valued key stores in place"
    );
}

#[test]
fn gc_dead_key_marking_clears() {
    for gen in [false, true] {
        let mut h = Heap::new(0).unwrap();
        if gen {
            h.change_mode(&mut |_m: &mut Marker| {}, GcKind::GenMinor);
            assert!(h.gc_kind().is_generational());
        }
        let (t, keys) = table_with(&mut h, 16, 12, "g");
        // a full collection that finds nothing to clear keeps the id
        stamp(t);
        collect_with(&mut h, &[Value::table(t)]);
        assert_eq!(
            t.layout_id(),
            MARK,
            "collection with nothing dead (gen={gen})"
        );
        // an entry with a nil value: its (collectable) key becomes a dead key
        t.set(&mut h, &keys[3], &Value::NIL).unwrap();
        assert_eq!(t.layout_id(), MARK);
        let before = layout(t);
        collect_with(&mut h, &[Value::table(t)]);
        assert_ne!(layout(t), before, "the key was marked dead");
        assert_eq!(t.layout_id(), 0, "dead key marking clears (gen={gen})");
        // a key that is not collectable (an integer) never changes: the id survives
        let t2 = h.new_table().unwrap();
        t2.resize(&mut h, 0, 8).unwrap();
        for i in 0..5 {
            t2.set(&mut h, &Value::int(1000 + i), &Value::int(1))
                .unwrap();
        }
        t2.set(&mut h, &Value::int(1001), &Value::NIL).unwrap();
        stamp(t2);
        let before = layout(t2);
        collect_with(&mut h, &[Value::table(t), Value::table(t2)]);
        assert_eq!(layout(t2), before);
        assert_eq!(t2.layout_id(), MARK, "integer keys are never dead-keyed");
    }
}

fn set_mode(h: &mut Heap, t: TableRef, mode: &str) {
    let mt = h.new_table().unwrap();
    let k = s(h, "__mode");
    let v = s(h, mode);
    mt.set(h, &k, &v).unwrap();
    t.set_metatable(Some(mt));
}

#[test]
fn weak_table_clears_clear_the_id() {
    for gen in [false, true] {
        for mode in ["v", "k", "kv"] {
            let mut h = Heap::new(0).unwrap();
            if gen {
                h.change_mode(&mut |_m: &mut Marker| {}, GcKind::GenMinor);
            }
            let t = h.new_table().unwrap();
            t.resize(&mut h, 0, 16).unwrap();
            set_mode(&mut h, t, mode);
            let mt = t.metatable().unwrap();
            // string keys with collectable (table) values and table keys with table values; none of the inner tables is rooted
            let mut dead = Vec::new();
            for i in 0..6 {
                let inner = h.new_table().unwrap();
                dead.push(inner);
                let key = s(&mut h, &format!("w{i}"));
                t.set(&mut h, &key, &Value::table(inner)).unwrap();
                let inner2 = h.new_table().unwrap();
                t.set(&mut h, &Value::table(inner2), &Value::table(inner))
                    .unwrap();
            }
            let live_key = h.new_table().unwrap();
            t.set(&mut h, &Value::table(live_key), &Value::int(1))
                .unwrap();
            let roots = [Value::table(t), Value::table(mt), Value::table(live_key)];
            // first collection: weak entries are emptied (and, for weak keys, node keys become dead in the same cycle)
            stamp(t);
            let before = layout(t);
            collect_with(&mut h, &roots);
            let after1 = layout(t);
            let changed1 = before != after1;
            if changed1 {
                assert_eq!(
                    t.layout_id(),
                    0,
                    "mode {mode} gen={gen}: tags changed, id kept"
                );
            }
            // second collection: whatever the first left empty is dead-keyed now
            stamp(t);
            let before = layout(t);
            collect_with(&mut h, &roots);
            let after2 = layout(t);
            if before != after2 {
                assert_eq!(t.layout_id(), 0, "mode {mode} gen={gen}: second cycle");
            } else {
                assert_eq!(t.layout_id(), MARK);
            }
            assert!(
                changed1 || before != after2,
                "mode {mode}: no node key was cleared by the weak table collections"
            );
        }
    }
}

/// Random operations on tables with >= 8 nodes against a shadow model of (node index -> key tag/bits): whenever the layout
/// changed the id must be 0; whenever the operation cannot change it (value write of a live key, array write, read) the stamp must
/// survive. Sizes are small; strings (collectable, dead-keyable), integers and floats as keys; full collections in incremental and
/// generational mode; weak tables.
fn random_run(seed: u64, gen: bool, weak: Option<&str>, ops: usize) {
    let mut h = Heap::new(seed as u32).unwrap();
    if gen {
        h.change_mode(&mut |_m: &mut Marker| {}, GcKind::GenMinor);
    }
    let mut r = Rng(seed ^ 0x9E37_79B9);
    let t = h.new_table().unwrap();
    t.resize(&mut h, 4, 8 << r.below(3)).unwrap();
    if let Some(m) = weak {
        set_mode(&mut h, t, m);
    }
    let mt = t.metatable();
    let mut pool: Vec<Value> = Vec::new();
    for i in 0..24 {
        pool.push(s(&mut h, &format!("key{i}")));
    }
    for i in 0..8 {
        pool.push(Value::int(100 + 7 * i));
    }
    pool.push(Value::float(2.5));
    pool.push(Value::float(-0.25));
    pool.push(Value::TRUE);
    // keys of the pool must stay alive across the collections (strings are only referenced by the pool otherwise)
    let anchor = h.new_table().unwrap();
    for (i, k) in pool.iter().enumerate() {
        anchor.set_int(&mut h, i as i64 + 1, k).unwrap();
    }
    let mut roots = vec![Value::table(t), Value::table(anchor)];
    if let Some(m) = mt {
        roots.push(Value::table(m));
    }
    let mut kinds = [0usize; 8];
    for step in 0..ops {
        let prev = layout(t);
        stamp(t);
        let kind = r.below(8) as usize;
        kinds[kind] += 1;
        // `true`: the operation cannot change the layout
        let mut pure = false;
        match kind {
            0 | 1 => {
                // insert or overwrite a pool key; tables as weak values for the weak modes
                let k = pool[r.below(pool.len() as u64) as usize];
                let v = if weak.is_some() && r.below(2) == 0 {
                    Value::table(h.new_table().unwrap())
                } else {
                    Value::int(r.below(5) as i64 + 1)
                };
                t.set(&mut h, &k, &v).unwrap();
            }
            2 => {
                // nil store of a pool key: the layout cannot change (no insert, no GC)
                let k = pool[r.below(pool.len() as u64) as usize];
                t.set(&mut h, &k, &Value::NIL).unwrap();
                pure = true;
            }
            3 => {
                // value write of an existing live key
                let k = pool[r.below(pool.len() as u64) as usize];
                if !t.get(&k).is_nil() {
                    t.set(&mut h, &k, &Value::int(r.below(9) as i64 + 1))
                        .unwrap();
                    pure = true;
                }
            }
            4 => {
                // array part write / read
                let i = r.below(4) as i64 + 1;
                if t.array_size() >= 4 {
                    t.set_int(&mut h, i, &Value::int(i)).unwrap();
                    let _ = t.get_int(i);
                    pure = true;
                }
            }
            5 => {
                collect_with(&mut h, &roots);
            }
            6 => {
                // explicit resize
                let mut live = 0u32;
                let mut k = Value::NIL;
                while let Some((nk, _)) = t.next(&k).unwrap() {
                    live += 1;
                    k = nk;
                }
                let n = (8u32 << r.below(3)).max(live.next_power_of_two());
                t.resize(&mut h, 4, n).unwrap();
            }
            _ => {
                // reads
                for k in pool.iter().take(10) {
                    let _ = t.get(k);
                }
                let _ = t.len(&h);
                pure = true;
            }
        }
        let now = layout(t);
        if now != prev {
            assert!(
                !pure,
                "seed {seed} step {step} kind {kind}: a pure operation changed the layout"
            );
            if now.0 >= 8 {
                assert_eq!(
                    t.layout_id(),
                    0,
                    "seed {seed} gen={gen} weak={weak:?} step {step} kind {kind}: layout changed, id kept"
                );
            }
        } else if pure && now.0 >= 8 {
            assert_eq!(
                t.layout_id(),
                MARK,
                "seed {seed} step {step} kind {kind}: pure operation cleared the id"
            );
        }
        if now.0 < 8 {
            assert_eq!(t.layout_id(), 0);
        }
    }
    assert!(
        kinds.iter().all(|&c| c > 0),
        "kinds not all exercised: {kinds:?}"
    );
}

#[test]
fn randomized_layout_model() {
    let ops = if cfg!(miri) { 40 } else { 400 };
    let seeds = if cfg!(miri) { 2 } else { 24 };
    for seed in 1..=seeds {
        for gen in [false, true] {
            for weak in [None, Some("v"), Some("k"), Some("kv")] {
                random_run(seed, gen, weak, ops);
            }
        }
    }
}

#[test]
fn byte_accounting_includes_the_slot() {
    // 8 nodes: 24 * 8 + 8 (lastfree) counted; the id slot is allocated but NOT counted (the reference's accounting is kept, S4)
    // (small parts: 5.5 no lastfree; 5.4 lastfree only)
    let node = std::mem::size_of::<crate::table::Node>();
    let base = std::mem::size_of::<crate::table::Table>();
    let extra_small = if cfg!(rustynail_lua54) { 8 } else { 0 };
    assert_eq!(
        crate::table::new_table_bytes(0, 4),
        Some(base + 4 * node + extra_small)
    );
    assert_eq!(
        crate::table::new_table_bytes(0, 8),
        Some(base + 8 * node + 8)
    );
    assert_eq!(
        crate::table::new_table_bytes(0, 64),
        Some(base + 64 * node + 8)
    );
    let mut h = Heap::new(0).unwrap();
    for n in [1u32, 4, 8, 16, 32, 64] {
        let before = h.total_bytes();
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, n).unwrap();
        assert_eq!(
            h.total_bytes() - before,
            crate::table::new_table_bytes(0, n).unwrap()
        );
    }
    // freeing returns exactly what was allocated
    let mut h = Heap::new(0).unwrap();
    let base_bytes = h.total_bytes();
    for _ in 0..3 {
        collect_with(&mut h, &[]);
        let t = h.new_table().unwrap();
        t.resize(&mut h, 3, 16).unwrap();
        t.resize(&mut h, 0, 64).unwrap();
        let _ = t;
    }
    collect_with(&mut h, &[]);
    collect_with(&mut h, &[]);
    assert_eq!(h.total_bytes(), base_bytes);
}
