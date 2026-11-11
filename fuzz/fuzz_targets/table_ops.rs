//! Random table operations against a model (`HashMap`): contents must always agree and `next`
//! must enumerate each live entry exactly once.
#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rn_core::heap::Heap;
use rn_core::value::Value;
use std::collections::HashMap;

#[derive(Arbitrary, Debug)]
enum Op {
    SetInt(i16, i32),
    SetStr(u8, i32),
    SetFloat(i8, i32),
    Remove(i16),
    RemoveStr(u8),
    GetInt(i16),
    Resize(u8, u8),
    Len,
    Clear,
    Iterate,
    Collect,
}

#[derive(Arbitrary, Debug)]
struct Input {
    ops: Vec<Op>,
}

fuzz_target!(|inp: Input| {
    let mut h = Heap::new(7).unwrap();
    let t = h.new_table().unwrap();
    // anchor the table: this heap has no roots but the closure passed to `collect`
    let mut ints: HashMap<i64, i64> = HashMap::new();
    let mut strs: HashMap<Vec<u8>, i64> = HashMap::new();
    let mut flts: HashMap<i64, i64> = HashMap::new(); // float key = k + 0.5
    for op in inp.ops.iter().take(2000) {
        match *op {
            Op::SetInt(k, v) => {
                t.set_int(&mut h, k as i64, &Value::int(v as i64)).unwrap();
                ints.insert(k as i64, v as i64);
            }
            Op::SetStr(k, v) => {
                let key = vec![b'k', k];
                let ks = Value::string(h.new_str(&key).unwrap());
                t.set(&mut h, &ks, &Value::int(v as i64)).unwrap();
                strs.insert(key, v as i64);
            }
            Op::SetFloat(k, v) => {
                t.set(&mut h, &Value::float(k as f64 + 0.5), &Value::int(v as i64)).unwrap();
                flts.insert(k as i64, v as i64);
            }
            Op::Remove(k) => {
                t.set_int(&mut h, k as i64, &Value::NIL).unwrap();
                ints.remove(&(k as i64));
            }
            Op::RemoveStr(k) => {
                let key = vec![b'k', k];
                let ks = Value::string(h.new_str(&key).unwrap());
                t.set(&mut h, &ks, &Value::NIL).unwrap();
                strs.remove(&key);
            }
            Op::GetInt(k) => {
                assert_eq!(t.get_int(k as i64).as_int(), ints.get(&(k as i64)).copied());
            }
            Op::Resize(a, hsz) => {
                // `resize` is the pre-sizing primitive (`lua_createtable`): only valid while the
                // table is empty
                if ints.is_empty() && strs.is_empty() && flts.is_empty() {
                    t.resize(&mut h, a as u32, hsz as u32).ok();
                }
            }
            Op::Clear => {
                // `table.clear`: every entry gone, capacity and node layout untouched
                let (asz, hsz) = (t.array_size(), t.hash_size());
                t.clear_values();
                ints.clear();
                strs.clear();
                flts.clear();
                assert_eq!((t.array_size(), t.hash_size()), (asz, hsz));
                assert!(t.next(&Value::NIL).expect("valid next").is_none());
            }
            Op::Len => {
                let n = t.len(&h) as i64;
                // a border: t[n] non-nil (or n == 0) and t[n + 1] nil
                assert!(n == 0 || ints.contains_key(&n), "border {n} not present");
                assert!(!ints.contains_key(&(n + 1)), "border {n} but {} is present", n + 1);
            }
            Op::Iterate => {
                let mut seen = 0usize;
                let mut key = Value::NIL;
                while let Some((k, v)) = t.next(&key).expect("valid next") {
                    seen += 1;
                    assert!(v.as_int().is_some());
                    key = k;
                    assert!(seen <= ints.len() + strs.len() + flts.len(), "next enumerates too much");
                }
                assert_eq!(seen, ints.len() + strs.len() + flts.len());
            }
            Op::Collect => {
                let tv = Value::table(t);
                h.collect(&mut |m: &mut rn_core::gc::Marker| m.mark_value(&tv));
            }
        }
    }
    for (k, v) in &ints {
        assert_eq!(t.get_int(*k).as_int(), Some(*v));
    }
    let tv = Value::table(t);
    h.collect(&mut |m: &mut rn_core::gc::Marker| m.mark_value(&tv));
    h.verify();
});
