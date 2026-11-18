//! S4: the layout-id slot of hash parts with at least 8 nodes (rn-core `TableRef::layout_id`).
//!
//! Detectors of the mutants `shape_stamp_not_bumped_on_newkey`, `shape_collide_move_no_clear`, `shape_gc_deadkey_reuse`,
//! `shape_weak_clear_skip` (the rn-core clears; the unit tests of `rn-core/src/tests_shape.rs` are the full spec, these are the
//! same scenarios in miniature) and `shape_nr_id_skip` (the inline `NEWREF` of a trace store 0 to the slot: observed through a
//! nonzero "born" value of the slot, `set_mut_born_mark`).
#![cfg(feature = "trace")]

use rn_core::gc::{GcKind, Marker};
use rn_core::heap::Heap;
use rn_core::table::TableRef;
use rn_core::value::Value;

const MARK: u64 = 0x5EED_0000_0000_0001;

fn init() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(rn_jit::Config::for_mode(rn_jit::Mode::Always)),
            "the compiler must be installed by this test binary"
        );
        rn_jit::tr::install_driver();
        #[cfg(feature = "tr_mutants")]
        rn_jit::tr::mutants::apply_vm_mutants();
    });
}

fn s(h: &mut Heap, text: &str) -> Value {
    Value::string(h.new_str(text.as_bytes()).unwrap())
}

fn collect_with(h: &mut Heap, roots: &[Value]) {
    h.collect(&mut |m: &mut Marker| {
        for v in roots {
            m.mark_value(v);
        }
    });
}

fn stamp(t: TableRef) {
    assert!(t.set_layout_id(MARK));
}

#[test]
fn shape_clear_on_insert_and_collision() {
    init();
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    t.resize(&mut h, 0, 8).unwrap();
    // every insert into the 8-node part clears; the eight keys collide (free main position and collision paths both run)
    let mut collided = false;
    for i in 0..8 {
        stamp(t);
        let k = s(&mut h, &format!("key{i}"));
        t.set(&mut h, &k, &Value::int(1)).unwrap();
        assert_eq!(t.layout_id(), 0, "insert {i}");
        collided |= (0..8).any(|j| t.node_next(j) != 0);
    }
    assert!(collided);
    // the same with integer keys until a collision moves a live key
    let mut moved = false;
    let mut rng = 12345u64;
    for seed in 1..200u32 {
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, 8).unwrap();
        for j in 0..8i64 {
            let before: Vec<_> = (0..8).map(|i| t.node_key(i)).collect();
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let key = Value::int(100 + (rng >> 33) as i64 % 1000);
            if !t.get(&key).is_nil() {
                continue; // present: a value write, not an insertion
            }
            stamp(t);
            t.set(&mut h, &key, &Value::int(1)).unwrap();
            if t.hash_size() != 8 {
                break;
            }
            let after: Vec<_> = (0..8).map(|i| t.node_key(i)).collect();
            moved |= (0..8).any(|i| before[i].0 != 0 && before[i] != after[i]);
            assert_eq!(t.layout_id(), 0, "int insert seed {seed} #{j}");
        }
    }
    assert!(moved, "no collision move exercised");
}

#[test]
fn shape_clear_on_gc_dead_key_and_weak_clears() {
    init();
    for gen in [false, true] {
        let mut h = Heap::new(0).unwrap();
        if gen {
            h.change_mode(&mut |_m: &mut Marker| {}, GcKind::GenMinor);
        }
        // strong table: a nil-valued entry's collectable key becomes a dead key
        let t = h.new_table().unwrap();
        t.resize(&mut h, 0, 16).unwrap();
        let mut keys = Vec::new();
        for i in 0..12 {
            let k = s(&mut h, &format!("g{i}"));
            t.set(&mut h, &k, &Value::int(1)).unwrap();
            keys.push(k);
        }
        stamp(t);
        collect_with(&mut h, &[Value::table(t)]);
        assert_eq!(t.layout_id(), MARK, "nothing dead: the id survives");
        t.set(&mut h, &keys[3], &Value::NIL).unwrap();
        assert_eq!(t.layout_id(), MARK, "nil store keeps the id");
        collect_with(&mut h, &[Value::table(t)]);
        assert_eq!(t.layout_id(), 0, "dead key marking (gen={gen})");
        // weak tables: unrooted table values / keys are cleared, then their keys die
        for mode in ["v", "k", "kv"] {
            let w = h.new_table().unwrap();
            w.resize(&mut h, 0, 16).unwrap();
            let mt = h.new_table().unwrap();
            let mk = s(&mut h, "__mode");
            let mv = s(&mut h, mode);
            mt.set(&mut h, &mk, &mv).unwrap();
            w.set_metatable(Some(mt));
            for i in 0..6 {
                let inner = h.new_table().unwrap();
                let k = s(&mut h, &format!("w{i}"));
                w.set(&mut h, &k, &Value::table(inner)).unwrap();
                let k2 = h.new_table().unwrap();
                w.set(&mut h, &Value::table(k2), &Value::table(inner))
                    .unwrap();
            }
            let roots = [Value::table(w), Value::table(mt)];
            stamp(w);
            collect_with(&mut h, &roots);
            let first = w.layout_id();
            stamp(w);
            collect_with(&mut h, &roots);
            let second = w.layout_id();
            assert!(
                first == 0 || second == 0,
                "weak mode {mode} gen={gen}: collections that cleared entries kept the id"
            );
        }
    }
}

/// `shape_nr_id_skip`: constructors of 5 to 8 constant keys (8-node hash parts) built by a trace through the inline `NEWREF`; with
/// a nonzero born value of the slot, every table must end with 0.
#[cfg(feature = "tr_mutants")]
#[test]
fn shape_trace_newref_clears_the_id() {
    use rn_core::state::{LuaState, LUA_MULTRET};
    use rn_core::status::Status;
    use rn_jit::rt::set_trace;
    use rn_jit::t15::runtime::{T15Config, T15Mode};
    use rn_jit::tr::cache::{TrConfig, TrMode};
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, rn_jit::Mode::Always);
    rn_jit::set_t15(&mut st, T15Config::for_mode(T15Mode::Off));
    set_trace(&mut st, TrConfig::for_mode(TrMode::Always));
    rn_jit::tr::attach_driver(&mut st);
    let src = "
      KEEP = {}
      local function run(n)
        local acc = 0
        for i = 1, n do
          local a = {name = i, value = 2, key = 3, next = 4, len = 5, size = 6, id = 7, tag = 8}
          local b = {k1 = i, k2 = 2, k3 = 3, k4 = 4, k5 = 5}
          local c = {x = i, y = 2, z = 3, w = 4, v = 5, u = 6}
          KEEP[i % 3 + 1] = a KEEP[i % 3 + 4] = b KEEP[i % 3 + 7] = c
          acc = acc + a.name + b.k1 + c.x
        end
        return acc
      end
      return run(3000) + run(7)";
    // the tables are born with the marker in their slot: only the clear on insertion can remove it
    rn_core::table::set_mut_born_mark(MARK);
    let base = st.top();
    assert_eq!(st.load_buffer(src.as_bytes(), b"=shape", None), Status::Ok);
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    rn_core::table::set_mut_born_mark(0);
    assert_eq!(status, Status::Ok, "script failed: {:?}", st.get(func));
    st.set_top(base);
    let g = st.globals();
    let key = Value::string(st.heap().new_str(b"KEEP").unwrap());
    let keep = g.get(&key);
    let keep = keep.as_table().expect("KEEP");
    for i in 1..=9 {
        let t = keep.get_int(i).as_table().expect("kept table");
        assert!(t.hash_size() >= 8, "table {i}: {} nodes", t.hash_size());
        assert_eq!(
            t.layout_id(),
            0,
            "table {i}: the id survived the key insertions"
        );
    }
}

/// The library paths (`table.insert` / `table.remove` / `table.move`, `rawset`, `next`/`pairs`, `table.sort`) through the real
/// interpreter: array-part moves and value writes keep a stamped id, an insertion of a new hash key clears it.
#[test]
fn shape_library_paths() {
    use rn_core::state::{LuaState, LUA_MULTRET};
    use rn_core::status::Status;
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, rn_jit::Mode::Off);
    let run = |st: &mut LuaState, src: &str| {
        let base = st.top();
        assert_eq!(st.load_buffer(src.as_bytes(), b"=shape", None), Status::Ok);
        let func = st.top() - 1;
        assert_eq!(st.pcall(func, LUA_MULTRET, 0), Status::Ok, "{src}");
        st.set_top(base);
    };
    let get = |st: &mut LuaState| {
        let g = st.globals();
        let key = Value::string(st.heap().new_str(b"T").unwrap());
        g.get(&key).as_table().expect("T")
    };
    let fresh = "local s = {} for i = 1, 8 do s[i] = i end \
        for i = 1, 8 do s['h' .. i] = i end T = s";
    let cases: &[(&str, bool)] = &[
        // (code, the stamp must survive)
        ("T[3] = 33 T[8] = nil T[8] = 8", true),
        ("T.h2 = 5 T.h2 = nil T.h2 = 6", true),
        ("table.remove(T, 2) table.insert(T, 2, 'x')", true),
        ("table.move(T, 1, 4, 2)", true),
        ("table.sort(T)", true),
        (
            "local n = 0 for k, v in pairs(T) do n = n + 1 end local m = #T local u = T.h1 + T.h8",
            true,
        ),
        (
            "rawset(T, 2, 7) rawset(T, 'h3', 7) local x = rawget(T, 'h3')",
            true,
        ),
        ("T.fresh1 = 1", false),
        ("rawset(T, 'fresh2', 1)", false),
        ("T[1000] = 1", false),
        ("T[1.5] = 1", false),
        ("T[true] = 1", false),
        (
            "table.insert(T, 'p1') table.insert(T, 'p2') table.insert(T, 'p3')",
            false,
        ),
        ("for i = 1, 40 do T['n' .. i] = i end", false),
    ];
    for (code, survives) in cases {
        run(&mut st, fresh);
        let t = get(&mut st);
        assert!(t.hash_size() >= 8);
        stamp(t);
        run(&mut st, code);
        let t = get(&mut st);
        if *survives {
            assert_eq!(t.layout_id(), MARK, "{code}: the id must survive");
        } else {
            assert_eq!(t.layout_id(), 0, "{code}: the id must be cleared");
        }
    }
}

/// S4: the layout ids of `rn_jit::tr::shape` are content addressed: equal layouts of different tables get one id, different layouts different ids, an
/// insertion clears the slot and the next computation finds the new layout's id, parts without a slot (fewer than eight nodes) and parts that are too large
/// get none. Detector of the mutant `shape_id_reuse` (an id that names two layouts).
#[test]
fn shape_ids_are_content_addressed() {
    use rn_jit::tr::shape::layout_id;
    init();
    let mut h = Heap::new(0).unwrap();
    let build = |h: &mut Heap, keys: &[&str], nodes: u32| {
        let t = h.new_table().unwrap();
        t.resize(h, 0, nodes).unwrap();
        for (i, k) in keys.iter().enumerate() {
            let k = s(h, k);
            t.set(h, &k, &Value::int(i as i64)).unwrap();
        }
        t
    };
    let base = ["x", "y", "z", "vx", "vy", "vz", "mass"];
    let a = build(&mut h, &base, 8);
    let b = build(&mut h, &base, 8);
    assert_eq!(a.layout_id(), 0, "born 0");
    let ia = layout_id(a);
    assert!(ia != 0 && ia < 1 << 31, "{ia}");
    assert_eq!(a.layout_id(), ia, "stored in the slot");
    assert_eq!(layout_id(b), ia, "identical layouts, one id");
    assert_eq!(layout_id(a), ia, "stable");
    // other key sets and other node counts: other ids, pairwise different unless the layouts are equal
    let sets: Vec<Vec<&str>> = vec![
        base.to_vec(),
        vec!["x", "y", "z", "vx", "vy", "vz", "m2"],
        vec!["x", "y", "z", "vx", "vy", "vz"],
        vec!["x", "y", "z"],
        vec!["p", "q", "r", "s", "t", "u", "v", "w"],
        vec!["w", "v", "u", "t", "s", "r", "q", "p"],
        vec!["a", "b", "c", "d", "e", "f", "g", "h", "i"],
    ];
    let mut ids = Vec::new();
    for ks in &sets {
        let t = build(&mut h, ks, 8);
        let t2 = build(&mut h, ks, 16);
        ids.push((layout_id(t), t));
        ids.push((layout_id(t2), t2));
    }
    for (i, x) in ids.iter().enumerate() {
        for y in &ids[i + 1..] {
            let same = x.1.lsizenode() == y.1.lsizenode()
                && (0..1usize << x.1.lsizenode()).all(|n| x.1.node_key(n) == y.1.node_key(n));
            assert_eq!(
                x.0 == y.0,
                same,
                "ids {} {} ({} / {} nodes)",
                x.0,
                y.0,
                1 << x.1.lsizenode(),
                1 << y.1.lsizenode()
            );
        }
    }
    // an insertion clears the slot; the next computation interns the new layout
    let t = build(&mut h, &base, 16);
    let before = layout_id(t);
    assert_eq!(t.layout_id(), before);
    let k = s(&mut h, "late");
    t.set(&mut h, &k, &Value::int(1)).unwrap();
    assert_eq!(t.layout_id(), 0, "an insertion clears the id");
    let after = layout_id(t);
    assert!(after != 0 && after != before, "{before} {after}");
    // no slot below eight nodes, none above the size limit
    let small = build(&mut h, &["x", "y"], 2);
    assert_eq!(layout_id(small), 0);
    let big = h.new_table().unwrap();
    big.resize(&mut h, 0, 256).unwrap();
    assert_eq!(layout_id(big), 0);
}
