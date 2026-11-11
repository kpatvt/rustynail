//! Micro-benchmarks (run by hand): `cargo test --release -p rn-core --test bench_table -- --ignored --nocapture`
//! Prints ns/op; the same snippets are timed in Lua with the reference interpreter by `ref_lua_snippets`.
use rn_core::heap::Heap;
use rn_core::value::Value;
use std::time::Instant;

fn report(name: &str, n: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let ns = t0.elapsed().as_nanos() as f64 / n as f64;
    println!("BENCH {name:<34} {ns:8.2} ns/op  ({n} ops)");
}

#[test]
#[ignore]
fn bench_tables() {
    let mut h = Heap::new(0).unwrap();
    // 1. sequential integer insert
    let n = 10_000_000u64;
    let t = h.new_table().unwrap();
    report("seq insert t[i]=i (10M)", n, || {
        for i in 1..=n as i64 {
            t.set_int(&mut h, i, &Value::int(i)).unwrap();
        }
    });
    let mut sum = 0i64;
    report("seq get t[i] (10M)", n, || {
        for i in 1..=n as i64 {
            sum = sum.wrapping_add(t.get_int(i).as_int().unwrap());
        }
    });
    assert!(sum != 0);
    // 2. random get/set on a 1M-entry hash (keys are large, so they live in the hash part)
    let t2 = h.new_table().unwrap();
    let m = 1_000_000u64;
    let key = |i: u64| -> i64 { ((i.wrapping_mul(0x9E3779B97F4A7C15)) >> 8) as i64 | 1 << 40 };
    report("hash fill 1M int keys", m, || {
        for i in 0..m {
            t2.set_int(&mut h, key(i), &Value::int(i as i64)).unwrap();
        }
    });
    let ops = 5_000_000u64;
    let mut x = 12345u64;
    let mut hits = 0u64;
    report("hash random get (5M)", ops, || {
        for _ in 0..ops {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            if !t2.get_int(key((x >> 33) % m)).is_nil() {
                hits += 1;
            }
        }
    });
    assert_eq!(hits, ops);
    report("hash random set existing (5M)", ops, || {
        for _ in 0..ops {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            t2.set_int(&mut h, key((x >> 33) % m), &Value::int(1))
                .unwrap();
        }
    });
    // 3. string-key get
    let t3 = h.new_table().unwrap();
    let keys: Vec<Value> = (0..1000)
        .map(|i| Value::string(h.new_str(format!("field_{i}").as_bytes()).unwrap()))
        .collect();
    for (i, k) in keys.iter().enumerate() {
        t3.set(&mut h, k, &Value::int(i as i64)).unwrap();
    }
    let gets = 20_000_000u64;
    let mut acc = 0i64;
    report("string-key get (20M)", gets, || {
        for i in 0..gets as usize {
            acc = acc.wrapping_add(t3.get(&keys[i % 1000]).as_int().unwrap());
        }
    });
    assert!(acc > 0);
}

#[test]
#[ignore]
fn ref_lua_snippets() {
    let bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/55/build/lua");
    if !bin.exists() {
        eprintln!("NOTICE: reference lua missing");
        return;
    }
    let script = r#"
local clock = os.clock
local function bench(name, n, f) local t0 = clock(); f(); print(string.format("REF   %-34s %8.2f ns/op  (%d ops)", name, (clock()-t0)*1e9/n, n)) end
collectgarbage("stop")
local t = {}
bench("seq insert t[i]=i (10M)", 1e7, function() for i = 1, 1e7 do t[i] = i end end)
local s = 0
bench("seq get t[i] (10M)", 1e7, function() for i = 1, 1e7 do s = s + t[i] end end)
local t2 = {}
local m = 1000000
local function key(i) return ((i * -7046029254386353131) >> 8) | (1 << 40) end
bench("hash fill 1M int keys", m, function() for i = 0, m - 1 do t2[key(i)] = i end end)
local x = 12345
bench("hash random get (5M)", 5e6, function() for _ = 1, 5e6 do x = x * 6364136223846793005 + 1442695040888963407; local _ = t2[key((x >> 33) % m)] end end)
bench("hash random set existing (5M)", 5e6, function() for _ = 1, 5e6 do x = x * 6364136223846793005 + 1442695040888963407; t2[key((x >> 33) % m)] = 1 end end)
local t3, keys = {}, {}
for i = 0, 999 do keys[i + 1] = "field_" .. i; t3[keys[i + 1]] = i end
bench("string-key get (20M)", 2e7, function() local a = 0; for i = 0, 2e7 - 1 do a = a + t3[keys[i % 1000 + 1]] end end)
"#;
    let path = std::env::temp_dir().join("rustynail-bench.lua");
    std::fs::write(&path, script).unwrap();
    let out = std::process::Command::new(bin).arg(&path).output().unwrap();
    println!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
