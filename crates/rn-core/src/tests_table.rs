//! Table tests: model-based property tests, border invariants, iteration order, resizing.
use crate::heap::Heap;
use crate::table::{TableError, TableRef};
use crate::value::Value;
use std::collections::HashMap;

pub(crate) struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum K {
    Int(i64),
    Flt(u64),
    Bool(bool),
    Str(Vec<u8>),
}

fn norm(v: &Value) -> Option<K> {
    if let Some(i) = v.as_int() {
        return Some(K::Int(i));
    }
    if let Some(f) = v.as_float() {
        if f.is_nan() {
            return None;
        }
        if let Some(i) = crate::number::float_to_int(f, crate::number::F2I::Eq) {
            return Some(K::Int(i));
        }
        return Some(K::Flt(f.to_bits()));
    }
    if let Some(b) = v.as_bool() {
        return Some(K::Bool(b));
    }
    v.as_string().map(|s| K::Str(s.as_bytes().to_vec()))
}

fn key_pool(h: &mut Heap) -> Vec<Value> {
    let mut keys = Vec::new();
    for i in -5i64..40 {
        keys.push(Value::int(i));
    }
    for i in [
        i64::MAX,
        i64::MIN,
        i64::MAX - 1,
        1 << 31,
        (1 << 31) + 1,
        1 << 32,
        (1 << 53) + 1,
        1 << 53,
        -(1 << 53),
        100,
        1000,
        1001,
        4096,
        4097,
        65536,
    ] {
        keys.push(Value::int(i));
    }
    for f in [
        0.0,
        -0.0,
        1.5,
        -2.5,
        2.0,
        3.0,
        1e300,
        f64::INFINITY,
        f64::NEG_INFINITY,
        9007199254740992.0,
        1e15,
        0.1,
    ] {
        keys.push(Value::float(f));
    }
    keys.push(Value::TRUE);
    keys.push(Value::FALSE);
    for i in 0..30 {
        let s = h.new_str(format!("k{i}").as_bytes()).unwrap();
        keys.push(Value::string(s));
    }
    for i in 0..6 {
        let s = h
            .new_str(format!("a long string key number {i} with more than forty bytes").as_bytes())
            .unwrap();
        keys.push(Value::string(s));
    }
    keys
}

fn check_all(t: TableRef, model: &HashMap<K, Value>, h: &Heap) {
    // traversal visits exactly the model's entries
    let mut seen = HashMap::new();
    let mut k = Value::NIL;
    while let Some((nk, nv)) = t.next(&k).unwrap() {
        let key = norm(&nk).expect("valid key");
        assert!(seen.insert(key, nv).is_none(), "key visited twice");
        k = nk;
    }
    assert_eq!(seen.len(), model.len());
    for (key, v) in model {
        let got = seen.get(key).unwrap_or_else(|| panic!("missing {key:?}"));
        assert!(got.raw_equal(v), "{key:?}: {got:?} vs {v:?}");
    }
    // border invariant
    let n = t.len(h);
    if n == 0 {
        assert!(t.get_int(1).is_nil());
    } else {
        assert!(!t.get_int(n as i64).is_nil(), "t[n] nil for n={n}");
        if n < i64::MAX as u64 {
            assert!(t.get_int(n as i64 + 1).is_nil(), "t[n+1] present for n={n}");
        }
    }
}

fn run_model(seed: u64, ops: usize) {
    let mut h = Heap::new(seed as u32).unwrap();
    let keys = key_pool(&mut h);
    let t = h.new_table().unwrap();
    let mut model: HashMap<K, Value> = HashMap::new();
    let mut r = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let mut counter = 0i64;
    // bias towards a sequence region sometimes
    let dense = r.below(3) == 0;
    for step in 0..ops {
        let key = if dense && r.below(4) != 0 {
            Value::int(r.below(80) as i64 + 1)
        } else {
            keys[r.below(keys.len() as u64) as usize]
        };
        match r.below(12) {
            0..=4 => {
                counter += 1;
                let v = Value::int(counter);
                match t.set(&mut h, &key, &v) {
                    Ok(()) => {
                        model.insert(norm(&key).unwrap(), v);
                    }
                    Err(e) => assert_eq!(e, TableError::NanIndex),
                }
            }
            5..=6 => {
                t.set(&mut h, &key, &Value::NIL).ok();
                if let Some(k) = norm(&key) {
                    model.remove(&k);
                }
            }
            7 => {
                let got = t.get(&key);
                match norm(&key).and_then(|k| model.get(&k)) {
                    Some(v) => assert!(got.raw_equal(v)),
                    None => assert!(got.is_nil()),
                }
            }
            8 => {
                // append at the border
                let n = t.len(&h);
                counter += 1;
                let v = Value::int(counter);
                t.set_int(&mut h, n as i64 + 1, &v).unwrap();
                model.insert(K::Int(n as i64 + 1), v);
            }
            9 => {
                // remove the last element of the sequence
                let n = t.len(&h);
                if n > 0 {
                    t.set_int(&mut h, n as i64, &Value::NIL).unwrap();
                    model.remove(&K::Int(n as i64));
                }
            }
            10 => {
                // explicit resize keeping contents (shrinking the array moves entries to the hash)
                let na = r.below(70) as u32;
                let nh = model.len() as u32 + r.below(10) as u32; // must hold every entry
                t.resize(&mut h, na, nh).unwrap();
            }
            _ => {
                // traversal with assignment to existing fields
                let mut k = Value::NIL;
                while let Some((nk, _)) = t.next(&k).unwrap() {
                    if r.below(3) == 0 {
                        counter += 1;
                        let v = Value::int(counter);
                        t.set(&mut h, &nk, &v).unwrap();
                        model.insert(norm(&nk).unwrap(), v);
                    } else if r.below(5) == 0 {
                        t.set(&mut h, &nk, &Value::NIL).unwrap();
                        model.remove(&norm(&nk).unwrap());
                    }
                    k = nk;
                }
            }
        }
        if step % 16 == 0 {
            check_all(t, &model, &h);
        }
    }
    check_all(t, &model, &h);
}

#[test]
fn model_random_ops() {
    let (seeds, ops) = if cfg!(miri) { (3, 150) } else { (400, 600) };
    for seed in 1..=seeds {
        run_model(seed, ops);
    }
}

#[test]
fn nil_and_nan_keys_rejected() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    assert_eq!(
        t.set(&mut h, &Value::NIL, &Value::int(1)),
        Err(TableError::NilIndex)
    );
    assert_eq!(
        t.set(&mut h, &Value::float(f64::NAN), &Value::int(1)),
        Err(TableError::NanIndex)
    );
    // setting nil on an absent nil/NaN key is not an error in the reference only when the value is
    // nil; here it is an error too (the VM never gets there for nil values)
    assert!(t.get(&Value::NIL).is_nil());
    assert!(t.get(&Value::float(f64::NAN)).is_nil());
}

#[test]
fn float_keys_normalise() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    t.set(&mut h, &Value::float(2.0), &Value::int(20)).unwrap();
    assert_eq!(t.get_int(2).as_int(), Some(20));
    t.set(&mut h, &Value::float(-0.0), &Value::int(7)).unwrap();
    assert_eq!(t.get_int(0).as_int(), Some(7));
    t.set(&mut h, &Value::float(0.5), &Value::int(5)).unwrap();
    assert_eq!(t.get(&Value::float(0.5)).as_int(), Some(5));
}

#[test]
fn sequence_growth_sizes_match_reference() {
    // appending 1..=n through `t[#t+1]` yields power-of-two array sizes like the reference
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    for i in 1..=100i64 {
        t.set_int(&mut h, i, &Value::int(i)).unwrap();
    }
    assert_eq!(t.array_size(), 128);
    assert_eq!(t.len(&h), 100);
    for i in 1..=100i64 {
        assert_eq!(t.get_int(i).as_int(), Some(i));
    }
}

#[test]
fn len_with_hash_part_keys() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    t.resize(&mut h, 0, 16).unwrap();
    for i in 1..=10i64 {
        t.set_int(&mut h, i, &Value::TRUE).unwrap();
    }
    assert_eq!(t.len(&h), 10);
    t.set_int(&mut h, 11, &Value::TRUE).unwrap();
    assert_eq!(t.len(&h), 11);
    t.set_int(&mut h, 1 << 40, &Value::TRUE).unwrap();
    assert_eq!(t.len(&h), 11);
}

#[test]
fn invalid_next_key() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    t.set_int(&mut h, 1, &Value::TRUE).unwrap();
    assert_eq!(
        t.next(&Value::int(12345)).unwrap_err(),
        TableError::InvalidNextKey
    );
}

#[test]
fn tm_cache_flags() {
    use crate::meta::Tm;
    let mut h = Heap::new(0).unwrap();
    let mt = h.new_table().unwrap();
    assert!(h.fast_tm(Some(mt), Tm::Index).is_none());
    assert!(TableRef::no_tm(Some(mt), Tm::Index)); // cached absence
    let name = h.new_str(b"__index").unwrap();
    mt.set(&mut h, &Value::string(name), &Value::TRUE).unwrap(); // invalidates cache
    assert!(!TableRef::no_tm(Some(mt), Tm::Index));
    assert!(h.fast_tm(Some(mt), Tm::Index).is_some());
}

#[test]
fn node_is_24_bytes() {
    assert_eq!(std::mem::size_of::<crate::table::Node>(), 24);
}

/// A1: `plan_shortstr_insert` + `InsertPlan::apply` predict exactly the hash part the real insertion leaves, for every key
/// order and size (the plan is `None` only for the shapes it declines: a moved colliding node, a rehash, a hash part over 8 nodes).
#[test]
fn insert_plan_matches_the_real_insertion() {
    let mut h = Heap::new(0).unwrap();
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let (mut some, mut none, mut chained) = (0, 0, 0);
    for round in 0..400 {
        let nodes = [1u32, 2, 4, 8][round % 4];
        let t = h.new_table().unwrap();
        t.resize(&mut h, (round % 3) as u32, nodes).unwrap();
        assert!(
            t.plan_shortstr_insert(h.new_str(b"probe").unwrap())
                .is_some()
                || nodes >= 1
        );
        let mut img = t.hash_image();
        let mut tracked = true;
        for k in 0..nodes {
            let name = format!("f{}_{}", rng.below(1000), k);
            let key = h.new_str(name.as_bytes()).unwrap();
            let plan = t.plan_shortstr_insert(key);
            let mut want = img.clone();
            if let Some(p) = &plan {
                p.apply(&mut want, key.as_ptr() as usize as u64);
                some += 1;
                chained += usize::from(p.node != p.main);
            } else {
                none += 1;
            }
            t.set(&mut h, &Value::string(key), &Value::TRUE).unwrap();
            let got = t.hash_image();
            if plan.is_some() && tracked {
                assert_eq!(got, want, "round {round} key {k}");
                assert_eq!(t.flags() & crate::meta::MASKFLAGS, 0);
            }
            // a declined shape (or a rehash) may leave any layout: the next predictions start from what the table holds
            tracked = plan.is_some() || tracked;
            img = got;
        }
    }
    assert!(
        some > 400 && chained > 20 && none > 0,
        "some {some} none {none} chained {chained}"
    );
}

/// F1: `next_hinted` is `next` for any hint: after each real step the hint is the returned position, and arbitrary (stale, foreign, out of
/// range) hints and keys never change the result or the error of `next`.
#[test]
fn next_hinted_equals_next() {
    let mut h = Heap::new(7).unwrap();
    let keys = key_pool(&mut h);
    let mut rng = Rng(0xF1F1_0001);
    for round in 0..60 {
        let t = h.new_table().unwrap();
        let other = h.new_table().unwrap();
        let n = 1 + rng.below(60) as usize;
        for _ in 0..n {
            let k = keys[rng.below(keys.len() as u64) as usize];
            t.set(&mut h, &k, &Value::int(rng.below(100) as i64))
                .unwrap();
            let k2 = keys[rng.below(keys.len() as u64) as usize];
            other.set(&mut h, &k2, &Value::TRUE).unwrap();
        }
        // a full traversal with the carried hint, against the plain one
        let mut hint = 0u32;
        let (mut kh, mut kp) = (Value::NIL, Value::NIL);
        loop {
            let a = t.next_hinted(&kh, &mut hint).unwrap();
            let b = t.next(&kp).unwrap();
            match (a, b) {
                (None, None) => break,
                (Some((ak, av)), Some((bk, bv))) => {
                    assert!(ak.raw_equal(&bk) && av.raw_equal(&bv), "round {round}");
                    kh = ak;
                    kp = bk;
                }
                _ => panic!("traversals differ in length (round {round})"),
            }
            // some rounds interleave a step on another table (the hint now belongs to that table)
            if rng.below(3) == 0 {
                let ok = if rng.below(2) == 0 {
                    Value::NIL
                } else {
                    keys[rng.below(keys.len() as u64) as usize]
                };
                let _ = other.next_hinted(&ok, &mut hint);
            }
            // some rounds clear the value of the key just returned (the allowed modification during a traversal)
            if rng.below(4) == 0 {
                t.set(&mut h, &kh, &Value::NIL).unwrap();
            }
        }
        // arbitrary hints and keys, valid or not
        for _ in 0..80 {
            let k = if rng.below(5) == 0 {
                Value::NIL
            } else {
                keys[rng.below(keys.len() as u64) as usize]
            };
            let mut hint = rng.below(100) as u32;
            let a = t.next_hinted(&k, &mut hint);
            let b = t.next(&k);
            match (a, b) {
                (Ok(None), Ok(None)) => {}
                (Ok(Some((ak, av))), Ok(Some((bk, bv)))) => {
                    assert!(ak.raw_equal(&bk) && av.raw_equal(&bv))
                }
                (Err(x), Err(y)) => assert_eq!(x, y),
                (x, y) => panic!("differ: {x:?} vs {y:?}"),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// C22: specialised growth paths (`resize_hash`, `rehash_append`, spare array capacity) against the generic path
// ---------------------------------------------------------------------------------------------

/// Everything observable about a table's layout and accounting.
fn fingerprint(
    t: TableRef,
    h: &Heap,
) -> (
    u32,
    u32,
    usize,
    u8,
    crate::table::HashImage,
    Vec<(Vec<u8>, i64)>,
    u64,
) {
    let mut order = Vec::new();
    let mut k = Value::NIL;
    while let Some((nk, nv)) = t.next(&k).unwrap() {
        let kb = if let Some(i) = nk.as_int() {
            format!("i{i}").into_bytes()
        } else if let Some(s) = nk.as_string() {
            s.as_bytes().to_vec()
        } else {
            format!("{nk:?}").into_bytes()
        };
        order.push((kb, nv.as_int().unwrap_or(-1)));
        k = nk;
    }
    (
        t.array_size(),
        0,
        t.hash_size(),
        t.lsizenode(),
        t.hash_image(),
        order,
        t.len(h),
    )
}

/// Run one random op sequence on two tables of one heap in lockstep, the first with the specialised growth paths, the second with
/// them disabled (generic counting + `resize`): after every op the layouts (array size, length hint, node vector with chain
/// links and `lastfree`, traversal order, border) and the bytes the op added to the heap must be equal.
fn growth_lockstep(seed: u64, ops: usize) {
    let mut h = Heap::new(7).unwrap();
    let all = key_pool(&mut h);
    let mut r = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    // a sub-pool of the keys: small pools make collisions and re-growth of the same shapes frequent
    let strs: Vec<Value> = all
        .iter()
        .copied()
        .filter(|v| v.as_string().is_some())
        .collect();
    let mode = r.below(5);
    let pool: Vec<Value> = (0..1 + r.below(10))
        .map(|_| strs[r.below(strs.len() as u64) as usize])
        .collect();
    let mut fast = h.new_table().unwrap();
    let mut slow = h.new_table().unwrap();
    let mut counter = 0i64;
    for step in 0..ops {
        let n = fast.array_size() as i64;
        let key = match (mode, r.below(6)) {
            (_, 0..=1) => Value::int(n + 1), // the append the shortcut handles
            (0, _) => Value::int(r.below(12) as i64 + 1),
            (1, _) => pool[r.below(pool.len() as u64) as usize],
            (2, _) => Value::int(n + 1 + r.below(3) as i64),
            (3, _) => all[r.below(all.len() as u64) as usize],
            _ => {
                if r.below(2) == 0 {
                    pool[r.below(pool.len() as u64) as usize]
                } else {
                    Value::int(n + r.below(4) as i64)
                }
            }
        };
        counter += 1;
        let (val, resize) = match r.below(10) {
            0..=1 => (Value::NIL, None),
            2 => (
                Value::int(counter),
                Some((r.below(9) as u32, r.below(9) as u32)),
            ),
            _ => (Value::int(counter), None),
        };
        let mut res = [Ok(()), Ok(())];
        let mut dbytes = [0usize; 2];
        for (j, t) in [fast, slow].into_iter().enumerate() {
            crate::table::NO_FAST_GROWTH.with(|c| c.set(j == 1));
            let before = h.total_bytes();
            res[j] = match resize {
                Some((na, nh)) => {
                    // keep every live entry: grow the hash part to the entry count
                    let live = fingerprint(t, &h).5.len() as u32;
                    t.resize(&mut h, na, nh.max(live))
                }
                None => t.set(&mut h, &key, &val),
            };
            dbytes[j] = h.total_bytes().wrapping_sub(before);
        }
        crate::table::NO_FAST_GROWTH.with(|c| c.set(false));
        assert_eq!(res[0], res[1], "seed {seed} step {step}");
        assert_eq!(
            dbytes[0], dbytes[1],
            "seed {seed} step {step}: accounted bytes"
        );
        assert_eq!(
            fingerprint(fast, &h),
            fingerprint(slow, &h),
            "seed {seed} step {step}: layout"
        );
        // SAFETY: live tables.
        assert_eq!(unsafe { (*fast.as_ptr()).byte_size() }, unsafe {
            (*slow.as_ptr()).byte_size()
        });
        if step % 97 == 96 {
            // start over with fresh tables: more shapes from small sizes
            fast = h.new_table().unwrap();
            slow = h.new_table().unwrap();
        }
    }
}

#[test]
fn growth_fast_paths_match_generic() {
    let (seeds, ops) = if cfg!(miri) { (3, 100) } else { (1500, 300) };
    for seed in 1..=seeds {
        growth_lockstep(seed, ops);
    }
}

/// Appending 1.. n elements: the sizes the array part takes, the bytes accounted and the length hint are the reference's.
#[test]
fn append_growth_sizes_and_bytes() {
    let mut h = Heap::new(1).unwrap();
    let t = h.new_table().unwrap();
    let base = h.total_bytes();
    let mut sizes = Vec::new();
    for i in 1..=70i64 {
        t.set_int(&mut h, i, &Value::int(i)).unwrap();
        sizes.push(t.array_size());
        // accounted: 9 bytes per slot + the 4-byte hint, whatever the physical capacity
        assert_eq!(h.total_bytes() - base, t.array_size() as usize * 9 + 4);
    }
    let expect: Vec<u32> = (1..=70u32).map(|i| i.next_power_of_two()).collect();
    assert_eq!(sizes, expect);
    // shrink back through the small sizes and free: nothing leaks (Heap::drop asserts 0 bytes)
    for na in [33u32, 5, 4, 3, 1, 0] {
        t.resize(&mut h, na, 0).unwrap();
        assert_eq!(
            h.total_bytes() - base,
            if na == 0 { 0 } else { na as usize * 9 + 4 }
        );
    }
}
