//! Collector tests: reachability, cycles, weak tables, ephemerons, finalizer lists, stress.
use crate::gc::{apply_param, code_param, GcHeader, Marker, WHITEBITS};
use crate::heap::Heap;
use crate::tests_table::Rng;
use crate::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

fn collect_with(h: &mut Heap, roots: &[Value]) {
    h.collect(&mut |m: &mut Marker| {
        for v in roots {
            m.mark_value(v);
        }
    });
    h.verify();
}

/// Heap whose string table has settled to its steady-state size (the first cycles shrink it).
fn settled_heap(seed: u32) -> Heap {
    let mut h = Heap::new(seed).unwrap();
    settle(&mut h);
    h
}

/// Collect (with no roots beyond `roots`) until memory stops changing.
fn settle_with(h: &mut Heap, roots: &[Value]) {
    let mut last = usize::MAX;
    while h.total_bytes() != last {
        last = h.total_bytes();
        collect_with(h, roots);
    }
}

fn settle(h: &mut Heap) {
    settle_with(h, &[]);
}

fn s(h: &mut Heap, text: &str) -> Value {
    Value::string(h.new_str(text.as_bytes()).unwrap())
}

fn set_mode(h: &mut Heap, t: crate::table::TableRef, mode: &str) -> crate::table::TableRef {
    let mt = h.new_table().unwrap();
    let k = s(h, "__mode");
    let v = s(h, mode);
    mt.set(h, &k, &v).unwrap();
    t.set_metatable(Some(mt));
    mt
}

#[test]
fn params_match_reference_encoding() {
    assert_eq!(code_param(250), 0x54);
    assert_eq!(apply_param(code_param(250), 1000), 2500);
    assert_eq!(apply_param(code_param(200), 1000), 2000);
    assert_eq!(apply_param(code_param(100), 12345), 12345);
}

#[test]
fn unreachable_objects_are_freed_and_memory_returns_to_baseline() {
    let mut h = settled_heap(0);
    let base_bytes = h.total_bytes();
    let base_objs = h.object_count();
    let mut roots = Vec::new();
    for i in 0..50 {
        let t = h.new_table().unwrap();
        for j in 0..20 {
            t.set_int(&mut h, j + 1, &Value::int(j)).unwrap();
        }
        let k = s(&mut h, &format!("key number {i}"));
        let val = s(
            &mut h,
            &format!("a rather long value string that does not fit in short strings #{i}"),
        );
        t.set(&mut h, &k, &val).unwrap();
        roots.push(Value::table(t));
    }
    assert!(h.total_bytes() > base_bytes);
    collect_with(&mut h, &roots);
    // everything still alive and intact
    for (i, r) in roots.iter().enumerate() {
        let t = r.as_table().unwrap();
        assert_eq!(t.get_int(7).as_int(), Some(6));
        let k = s(&mut h, &format!("key number {i}"));
        let v = t.get(&k);
        assert!(v
            .as_string()
            .unwrap()
            .as_bytes()
            .ends_with(format!("#{i}").as_bytes()));
    }
    roots.clear();
    settle(&mut h);
    assert_eq!(h.total_bytes(), base_bytes);
    assert_eq!(h.object_count(), base_objs);
}

#[test]
fn cycles_are_collected() {
    let mut h = settled_heap(0);
    let base = h.total_bytes();
    let a = h.new_table().unwrap();
    let b = h.new_table().unwrap();
    a.set_int(&mut h, 1, &Value::table(b)).unwrap();
    b.set_int(&mut h, 1, &Value::table(a)).unwrap();
    a.set_metatable(Some(b)); // metatable cycle too
    collect_with(&mut h, &[Value::table(a)]);
    assert!(h.total_bytes() > base);
    settle(&mut h);
    assert_eq!(h.total_bytes(), base);
}

#[test]
fn strings_are_collected_and_removed_from_the_string_table() {
    let mut h = Heap::new(0).unwrap();
    let base_len = h.strtab_len();
    // (B3-1: the table shrinks only below size / 16 strings, and the 256 one-byte strings stay)
    for i in 0..8000 {
        h.new_str(format!("temporary {i}").as_bytes()).unwrap();
    }
    assert_eq!(h.strtab_len(), base_len + 8000);
    let grown = h.strtab_size();
    collect_with(&mut h, &[]);
    assert_eq!(h.strtab_len(), base_len);
    assert!(
        h.strtab_size() < grown,
        "string table should shrink after strings die"
    );
    // metamethod names are fixed and survive
    assert!(h.fast_tm(None, crate::meta::Tm::Index).is_none());
}

#[test]
fn weak_values_clear_collected_objects_but_not_strings_or_numbers() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    set_mode(&mut h, t, "v");
    let alive = h.new_table().unwrap();
    let dead = h.new_table().unwrap();
    let keepstr = s(&mut h, "a string value");
    t.set_int(&mut h, 1, &Value::table(alive)).unwrap();
    t.set_int(&mut h, 2, &Value::table(dead)).unwrap();
    t.set_int(&mut h, 3, &Value::int(42)).unwrap();
    t.set_int(&mut h, 4, &keepstr).unwrap();
    let hk = s(&mut h, "hashed");
    t.set(&mut h, &hk, &Value::table(dead)).unwrap();
    let hk2 = s(&mut h, "hashed-alive");
    t.set(&mut h, &hk2, &Value::table(alive)).unwrap();
    collect_with(&mut h, &[Value::table(t), Value::table(alive)]);
    assert!(t.get_int(1).raw_equal(&Value::table(alive)));
    assert!(
        t.get_int(2).is_nil(),
        "collected value must be cleared (array part)"
    );
    assert_eq!(t.get_int(3).as_int(), Some(42));
    assert!(t.get_int(4).is_string());
    assert!(
        t.get(&hk).is_nil(),
        "collected value must be cleared (hash part)"
    );
    assert!(t.get(&hk2).raw_equal(&Value::table(alive)));
    // the key of the cleared entry was made dead, not dangling
    let (cnt, mut k) = (std::cell::Cell::new(0), Value::NIL);
    while let Some((nk, _)) = t.next(&k).unwrap() {
        cnt.set(cnt.get() + 1);
        k = nk;
    }
    assert_eq!(cnt.get(), 4);
}

#[test]
fn weak_keys_are_ephemerons() {
    let mut h = Heap::new(0).unwrap();
    let e = h.new_table().unwrap();
    set_mode(&mut h, e, "k");
    let k1 = h.new_table().unwrap(); // rooted
    let k2 = h.new_table().unwrap(); // reachable only through e[k1] (value)
    let k3 = h.new_table().unwrap(); // reachable only through e[k2] (value)
    let lost = h.new_table().unwrap(); // key unreachable
    let selfref = h.new_table().unwrap(); // value refers to own key: must still die
    e.set(&mut h, &Value::table(k1), &Value::table(k2)).unwrap();
    e.set(&mut h, &Value::table(k2), &Value::table(k3)).unwrap();
    e.set(&mut h, &Value::table(lost), &Value::int(1)).unwrap();
    selfref.set_int(&mut h, 1, &Value::table(selfref)).unwrap();
    e.set(&mut h, &Value::table(selfref), &Value::table(selfref))
        .unwrap();
    collect_with(&mut h, &[Value::table(e), Value::table(k1)]);
    assert!(e.get(&Value::table(k1)).raw_equal(&Value::table(k2)));
    assert!(
        e.get(&Value::table(k2)).raw_equal(&Value::table(k3)),
        "ephemeron chain keeps k2 -> k3"
    );
    let mut n = 0;
    let mut k = Value::NIL;
    while let Some((nk, _)) = e.next(&k).unwrap() {
        n += 1;
        k = nk;
    }
    assert_eq!(n, 2, "entries with unreachable keys are removed");
    // dropping k1 collapses the chain
    collect_with(&mut h, &[Value::table(e)]);
    let k0 = e.next(&Value::NIL).unwrap();
    assert!(k0.is_none());
}

#[test]
fn weak_keys_and_values_both() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    set_mode(&mut h, t, "kv");
    let a = h.new_table().unwrap();
    let b = h.new_table().unwrap();
    t.set(&mut h, &Value::table(a), &Value::table(b)).unwrap();
    t.set(&mut h, &Value::table(a), &Value::table(b)).unwrap();
    t.set_int(&mut h, 1, &Value::table(b)).unwrap();
    // a rooted, b not: value cleared => entry gone
    collect_with(&mut h, &[Value::table(t), Value::table(a)]);
    assert!(t.next(&Value::NIL).unwrap().is_none());
    // key unreachable
    t.set(&mut h, &Value::table(a), &Value::int(5)).unwrap();
    collect_with(&mut h, &[Value::table(t)]);
    assert!(t.next(&Value::NIL).unwrap().is_none());
}

#[test]
fn finalizer_lists_and_resurrection() {
    let mut h = Heap::new(0).unwrap();
    let called: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    let resurrected: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let (c, r) = (called.clone(), resurrected.clone());
        h.set_finalizer_hook(Some(Box::new(move |_h: &mut Heap, o: *mut GcHeader| {
            c.borrow_mut().push(o as usize);
            // the object is a table: keep it alive (resurrect) by storing it in a root list
            // SAFETY: the collector hands out a live table.
            let t = unsafe { crate::table::TableRef::from_raw(o.cast()) };
            r.borrow_mut().push(Value::table(t));
        })));
    }
    let mt = h.new_table().unwrap();
    let gcname = s(&mut h, "__gc");
    mt.set(&mut h, &gcname, &Value::TRUE).unwrap();
    let o = h.new_table().unwrap();
    o.set_metatable(Some(mt));
    // SAFETY: `o` is live with metatable `mt`.
    unsafe { h.check_finalizer(o.as_ptr().cast(), Some(mt)) };
    // a plain table with the same metatable but registered *without* __gc must not be finalised
    let plain = h.new_table().unwrap();
    // weak tables referencing the object
    let wv = h.new_table().unwrap();
    set_mode(&mut h, wv, "v");
    wv.set_int(&mut h, 1, &Value::table(o)).unwrap();
    let wk = h.new_table().unwrap();
    set_mode(&mut h, wk, "k");
    wk.set(&mut h, &Value::table(o), &Value::int(7)).unwrap();
    let _ = plain;
    let roots = [Value::table(mt), Value::table(wv), Value::table(wk)];
    collect_with(&mut h, &roots);
    assert_eq!(called.borrow().len(), 1);
    assert_eq!(called.borrow()[0], o.as_ptr() as usize);
    // weak value was cleared *before* resurrection; weak key kept (object is being finalised)
    assert!(wv.get_int(1).is_nil());
    assert_eq!(wk.get(&Value::table(o)).as_int(), Some(7));
    assert_eq!(h.pending_finalizers(), 0);
    // resurrected object survives while rooted, and is not finalised twice
    let mut roots2 = roots.to_vec();
    roots2.extend(resurrected.borrow().iter().copied());
    collect_with(&mut h, &roots2);
    assert_eq!(called.borrow().len(), 1);
    assert!(o.metatable().is_some());
    // once unreferenced it is really freed (no finalizer, bit was cleared)
    let before = h.object_count();
    collect_with(&mut h, &roots);
    assert!(h.object_count() < before);
    assert_eq!(called.borrow().len(), 1);
    assert!(wk.next(&Value::NIL).unwrap().is_none());
}

#[test]
fn finalizers_run_at_close_for_all_objects() {
    let mut h = Heap::new(0).unwrap();
    let count = Rc::new(RefCell::new(0));
    let c = count.clone();
    h.set_finalizer_hook(Some(Box::new(move |_h: &mut Heap, _o: *mut GcHeader| {
        *c.borrow_mut() += 1
    })));
    let mt = h.new_table().unwrap();
    let gcname = s(&mut h, "__gc");
    mt.set(&mut h, &gcname, &Value::TRUE).unwrap();
    for _ in 0..5 {
        let o = h.new_table().unwrap();
        o.set_metatable(Some(mt));
        // SAFETY: live object with metatable mt.
        unsafe { h.check_finalizer(o.as_ptr().cast(), Some(mt)) };
    }
    // still referenced from the root set: not finalised by a collection
    collect_with(&mut h, &[Value::table(mt)]);
    assert_eq!(
        *count.borrow(),
        5,
        "unreachable objects with __gc are finalised once"
    );
    let o = h.new_table().unwrap();
    o.set_metatable(Some(mt));
    // SAFETY: live object with metatable mt.
    unsafe { h.check_finalizer(o.as_ptr().cast(), Some(mt)) };
    h.close();
    assert_eq!(*count.borrow(), 6);
}

#[test]
fn dead_unswept_short_string_is_resurrected_by_interning() {
    let mut h = Heap::new(0).unwrap();
    let st = h.new_str(b"resurrect me").unwrap();
    // simulate "dead but not yet swept": other-white mark
    let other = h.current_white ^ WHITEBITS;
    // SAFETY: live string.
    unsafe { (*st.as_ptr()).hdr.marked = ((*st.as_ptr()).hdr.marked & !WHITEBITS) | other };
    let again = h.new_str(b"resurrect me").unwrap();
    assert_eq!(again.as_ptr(), st.as_ptr());
    // SAFETY: live string.
    unsafe {
        assert_eq!(
            (*st.as_ptr()).hdr.marked & WHITEBITS,
            h.current_white & WHITEBITS
        )
    };
    h.verify();
}

#[test]
fn safepoint_threshold_and_stop() {
    let mut h = Heap::new(0).unwrap();
    h.set_gc_stress(false);
    let mut none = |_: &mut Marker| {};
    // the first safepoint collects (initial threshold is 0, like the reference's initial debt)
    assert!(h.check_gc(&mut none));
    assert!(!h.check_gc(&mut none), "below threshold after a cycle");
    let threshold = h.gc_threshold();
    assert!(threshold >= h.total_bytes());
    // allocate past the threshold
    let mut roots = Vec::new();
    while h.total_bytes() <= threshold {
        roots.push(Value::table(h.new_table().unwrap()));
    }
    h.gc_stop();
    assert!(!h.check_gc(&mut none));
    h.gc_restart();
    // 5.4: `restart` sets a zero debt (`luaE_setdebt(g, 0)`): the next allocation makes a step due
    #[cfg(rustynail_lua54)]
    roots.push(Value::table(h.new_table().unwrap()));
    let before = h.gc_cycles();
    assert!(h.check_gc(&mut |m: &mut Marker| {
        for v in &roots {
            m.mark_value(v);
        }
    }));
    assert_eq!(h.gc_cycles(), before + 1);
    // stress mode: every safepoint collects
    h.set_gc_stress(true);
    assert!(h.check_gc(&mut |m: &mut Marker| {
        for v in &roots {
            m.mark_value(v);
        }
    }));
}

#[test]
fn out_of_memory_leaves_heap_consistent() {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    let limit = h.total_bytes() + 4096;
    h.mem_mut().set_limit(limit);
    let mut failed = false;
    for i in 0..10_000 {
        if t.set_int(&mut h, i + 1, &Value::int(i)).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
    // the table is still usable and consistent
    let n = t.len(&h);
    assert!(n > 0);
    assert_eq!(t.get_int(n as i64).as_int(), Some(n as i64 - 1));
    h.verify();
}

/// Collector configuration a random workload runs under.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// A full stop-the-world collection at every safepoint.
    Full,
    /// One incremental step at every safepoint, with `verify()` after each.
    Inc,
    /// One young collection at every safepoint, with `verify()` after each.
    Gen,
}

#[test]
fn stress_random_workload_with_invariants() {
    let ops: usize = if cfg!(miri) {
        400
    } else {
        std::env::var("GCOPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(220_000)
    };
    random_workload(Mode::Full, ops);
}

#[test]
fn incremental_random_workload_with_invariants() {
    random_workload(Mode::Inc, workload_ops(6000));
}

#[test]
fn generational_random_workload_with_invariants() {
    random_workload(Mode::Gen, workload_ops(6000));
}

/// Operation count of the verified workloads (`GCOPS` overrides it; small under Miri).
fn workload_ops(default: usize) -> usize {
    if cfg!(miri) {
        300
    } else {
        std::env::var("GCOPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
}

fn random_workload(mode: Mode, ops: usize) {
    let mut h = settled_heap(0x1234);
    let base_bytes = h.total_bytes();
    let base_strtab = h.strtab_size();
    match mode {
        Mode::Full => h.set_gc_stress(true),
        Mode::Inc | Mode::Gen => {
            if mode == Mode::Gen {
                h.change_mode(&mut |_m: &mut Marker| {}, crate::gc::GcKind::GenMinor);
                assert!(h.gc_kind().is_generational());
            }
            h.gc.stress_steps = true;
            h.gc.verify_steps = true;
        }
    }
    // finalizable garbage: a shared rooted metatable with `__gc`, a hook that counts calls
    let finalized = Rc::new(RefCell::new(0usize));
    {
        let f = finalized.clone();
        h.set_finalizer_hook(Some(Box::new(move |_h: &mut Heap, _o: *mut GcHeader| {
            *f.borrow_mut() += 1;
        })));
    }
    let gc_mt = h.new_table().unwrap();
    let gcname = s(&mut h, "__gc");
    gc_mt.set(&mut h, &gcname, &Value::TRUE).unwrap();
    let mut made_finalizable = 0usize;
    let mut r = Rng(0xDEADBEEF);
    // roots: tables; shadow: expected int fields per root and expected string bytes
    struct Root {
        v: Value,
        ints: Vec<(i64, i64)>,
        strs: Vec<(String, Vec<u8>)>,
    }
    let mut roots: Vec<Root> = Vec::new();
    let mut checks = 0usize;
    for step in 0..ops {
        match r.below(10) {
            0 | 1 => {
                if roots.len() < 40 {
                    let t = h.new_table().unwrap();
                    if r.below(5) == 0 {
                        let mode = ["k", "v", "kv"][r.below(3) as usize];
                        set_mode(&mut h, t, mode);
                    }
                    roots.push(Root {
                        v: Value::table(t),
                        ints: Vec::new(),
                        strs: Vec::new(),
                    });
                }
            }
            2..=4 => {
                // int field
                if !roots.is_empty() {
                    let i = r.below(roots.len() as u64) as usize;
                    let t = roots[i].v.as_table().unwrap();
                    let (k, val) = (r.below(50) as i64 + 1, r.below(1000) as i64);
                    // weak-valued tables may hold ints fine (numbers are never cleared)
                    t.set_int(&mut h, k, &Value::int(val)).unwrap();
                    roots[i].ints.retain(|(kk, _)| *kk != k);
                    roots[i].ints.push((k, val));
                }
            }
            5..=6 => {
                // string field: key and value strings are created fresh (and so unrooted until stored)
                if !roots.is_empty() {
                    let i = r.below(roots.len() as u64) as usize;
                    let t = roots[i].v.as_table().unwrap();
                    let kname = format!("f{}", r.below(30));
                    let len = r.below(70) as usize;
                    let bytes: Vec<u8> = (0..len).map(|_| r.below(256) as u8).collect();
                    let kv = s(&mut h, &kname);
                    let vv = Value::string(h.new_str(&bytes).unwrap());
                    t.set(&mut h, &kv, &vv).unwrap();
                    // the mutator's write barrier (the raw table API leaves it to the caller)
                    h.barrier_back(t.as_ptr().cast(), &kv);
                    h.barrier_back(t.as_ptr().cast(), &vv);
                    roots[i].strs.retain(|(n, _)| *n != kname);
                    roots[i].strs.push((kname, bytes));
                }
            }
            7 => {
                // link another root into a table (cycles and sharing)
                if roots.len() > 1 {
                    let (a, b) = (
                        r.below(roots.len() as u64) as usize,
                        r.below(roots.len() as u64) as usize,
                    );
                    let ta = roots[a].v.as_table().unwrap();
                    ta.set_int(&mut h, 100 + r.below(5) as i64, &roots[b].v)
                        .unwrap();
                    h.barrier_back(ta.as_ptr().cast(), &roots[b].v);
                }
            }
            8 => {
                // drop a root (the shadow model drops its expectations with it); caution: other roots
                // may link to it, which is fine (still alive then)
                if !roots.is_empty() && r.below(3) == 0 {
                    let i = r.below(roots.len() as u64) as usize;
                    roots.swap_remove(i);
                    // forget the links (keys 100..105) so dropped roots really become garbage
                    for root in &roots {
                        let t = root.v.as_table().unwrap();
                        for k in 100..105 {
                            t.set_int(&mut h, k, &Value::NIL).unwrap();
                        }
                    }
                }
            }
            _ => {
                // garbage: unreachable tables with cycles and strings
                let a = h.new_table().unwrap();
                let b = h.new_table().unwrap();
                a.set_int(&mut h, 1, &Value::table(b)).unwrap();
                b.set_int(&mut h, 1, &Value::table(a)).unwrap();
                if r.below(4) == 0 {
                    a.set_metatable(Some(gc_mt));
                    // SAFETY: `a` is live and its metatable is `gc_mt`.
                    unsafe { h.check_finalizer(a.as_ptr().cast(), Some(gc_mt)) };
                    made_finalizable += 1;
                }
                let g = s(
                    &mut h,
                    &format!(
                        "garbage {step} with a long enough tail to avoid short interning ........"
                    ),
                );
                a.set_int(&mut h, 2, &g).unwrap();
            }
        }
        // the safepoint: full collection in stress mode
        let mut rs: Vec<Value> = roots.iter().map(|x| x.v).collect();
        rs.push(Value::table(gc_mt));
        h.check_gc(&mut |m: &mut Marker| {
            for v in &rs {
                m.mark_value(v);
            }
        });
        if step % 97 == 0 {
            h.verify();
            for root in &roots {
                let t = root.v.as_table().unwrap();
                let weak_v = t.metatable().is_some();
                for (k, val) in &root.ints {
                    assert_eq!(
                        t.get_int(*k).as_int(),
                        Some(*val),
                        "int field lost (step {step})"
                    );
                }
                for (name, bytes) in &root.strs {
                    let key = s(&mut h, name);
                    let got = t.get(&key);
                    if weak_v {
                        // strings are never removed from weak tables
                    }
                    let gs = got
                        .as_string()
                        .unwrap_or_else(|| panic!("string field {name} lost (step {step})"));
                    assert_eq!(
                        gs.as_bytes(),
                        &bytes[..],
                        "string contents corrupted (step {step})"
                    );
                    checks += 1;
                }
            }
        }
    }
    assert!(checks > 0 || cfg!(miri));
    roots.clear();
    // the metatable (the only object still referenced) goes away with the last collection
    let mut last = usize::MAX;
    while h.total_bytes() != last {
        last = h.total_bytes();
        h.collect(&mut |_m: &mut Marker| {});
        h.verify();
    }
    h.collect(&mut |_m: &mut Marker| {});
    assert_eq!(
        *finalized.borrow(),
        made_finalizable,
        "every finalizer ran once ({mode:?})"
    );
    // (B3-1: the string table keeps a grown size until fewer than size / 16 strings are live: compare without its slots)
    let slot = std::mem::size_of::<usize>();
    assert_eq!(
        h.total_bytes() - h.strtab_size() * slot,
        base_bytes - base_strtab * slot,
        "everything freed once unreachable ({mode:?})"
    );
    if mode == Mode::Full {
        assert!(h.gc_cycles() as usize >= ops);
    }
}

#[test]
fn drop_frees_tables_and_strings_without_collection() {
    let mut h = Heap::new(0).unwrap();
    for i in 0..100 {
        let t = h.new_table().unwrap();
        t.set_int(&mut h, 1, &Value::int(i)).unwrap();
        let k = s(&mut h, &format!("some key {i}"));
        t.set(&mut h, &k, &Value::TRUE).unwrap();
    }
    drop(h); // Heap::drop debug-asserts that all memory was returned
}

/// Lua 5.4 `genstep`: a program that only builds a live structure makes the major collection a
/// "bad collection" (it frees less than half of the growth), so the collector stays incremental
/// (`lastatomic != 0`, still reported as generational) until a collection traverses less than 9/8
/// of the previous one; then it returns to minor collections.
#[cfg(rustynail_lua54)]
#[test]
fn gen54_bad_collection_falls_back_to_incremental_and_returns() {
    use crate::gc::GcKind;
    let mut h = settled_heap(7);
    h.change_mode(&mut |_m: &mut Marker| {}, GcKind::GenMinor);
    assert_eq!(h.gc_kind(), GcKind::GenMinor);
    // a growing chain (each new node points to the previous one: no barrier needed); only the
    // newest node is a root
    let mut head = Value::NIL;
    let mut saw_bad = false;
    let mut saw_inc = false;
    let n = if cfg!(miri) { 3_000 } else { 60_000 };
    for i in 0..n {
        let t = h.new_table().unwrap();
        t.set_int(&mut h, 1, &head).unwrap();
        t.set_int(&mut h, 2, &Value::int(i)).unwrap();
        head = Value::table(t);
        let root = head;
        h.check_gc(&mut |m: &mut Marker| m.mark_value(&root));
        if h.gc.lastatomic != 0 {
            // (`genstep` leaves the mode generational; `stepgenfull` switches to incremental)
            saw_bad = true;
            saw_inc |= h.gc_kind() == GcKind::Incremental;
            assert!(
                h.is_decgcmodegen(),
                "a bad collection keeps generational mode in name"
            );
        }
    }
    assert!(
        saw_bad,
        "building a live structure produces bad collections"
    );
    assert!(saw_inc, "repeated bad collections stay in incremental mode");
    h.verify();
    // drop the structure and allocate garbage: a good collection returns to generational mode
    let mut back = false;
    for _ in 0..n {
        let t = h.new_table().unwrap();
        t.set_int(&mut h, 1, &Value::TRUE).unwrap();
        h.check_gc(&mut |_m: &mut Marker| {});
        if h.gc_kind() == GcKind::GenMinor && h.gc.lastatomic == 0 {
            back = true;
            break;
        }
    }
    assert!(back, "a good collection returns to minor collections");
    h.verify();
}

/// BUNDLE-A item 1: until the program sets `minormul` itself, the young-collection nursery is a multiple (at least 5x, at most 20x) of
/// what the parameter says; the reported parameter stays 20 and an explicit value is used as given.
#[cfg(not(rustynail_lua54))]
#[test]
fn bundle_a_nursery_scale_follows_the_minormul_parameter() {
    use crate::gc::{GcKind, GCP_MINORMUL};
    let mut h = settled_heap(11);
    // a live structure so that the bytes alive after the major collection are not zero
    let keep: Vec<Value> = (0..200)
        .map(|_| Value::table(h.new_table().unwrap()))
        .collect();
    h.change_mode(
        &mut |m: &mut Marker| {
            for v in &keep {
                m.mark_value(v);
            }
        },
        GcKind::GenMinor,
    );
    assert_eq!(h.gc_kind(), GcKind::GenMinor);
    assert_eq!(h.gc_params().get(GCP_MINORMUL), 20);
    let dflt = h.minor_debt_bytes();
    h.gc_params().set(GCP_MINORMUL, 20);
    let explicit = h.minor_debt_bytes();
    assert!(explicit > 0);
    assert!(dflt >= 5 * explicit && dflt <= 20 * explicit, "default {dflt} vs explicit {explicit}");
    h.gc_params().set(GCP_MINORMUL, 100);
    // (the parameter is stored in a floating-point byte: 100% of x is only about 5 * (20% of x))
    let hundred = h.minor_debt_bytes();
    assert!(hundred.abs_diff(5 * explicit) * 20 <= 5 * explicit, "an explicit value is used as given: {hundred} vs {explicit}");
    drop(keep);
}
