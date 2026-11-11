//! Differential test of `table.rs` against the reference interpreter with a fixed string-hash seed
//! (`ref/55/build-seed0/refrun`): after each phase of a generated script the table's length (`#t`)
//! and the exact `pairs` traversal order must be identical. Skipped with a notice if the binary is
//! missing. (Under `--cfg rustynail_lua54` against `ref/54/build-seed0/refrun`.)
use rn_core::heap::Heap;
use rn_core::table::TableRef;
use rn_core::value::Value;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Debug)]
enum Key {
    Int(i64),
    Flt(f64),
    Str(String),
    Bool(bool),
}

impl Key {
    fn lua(&self) -> String {
        match self {
            Key::Int(i) if *i == i64::MIN => "math.mininteger".into(),
            Key::Int(i) => format!("{i}"),
            Key::Flt(f) => format!("{f:?}"),
            Key::Str(s) => format!("{s:?}"),
            Key::Bool(b) => format!("{b}"),
        }
    }
}

#[derive(Debug)]
enum Op {
    Set(Key, Option<i64>),
    Append(i64),
    Pop,
    /// delete every m-th visited element during a traversal
    DelTrav(u64),
    /// rewrite every element during a traversal
    RewriteTrav,
    /// copy loop: for i=1,n do t[i]=i end
    Fill(u64),
    Dump,
}

struct Script {
    /// constructor: number of positional items, hash fields
    ctor_arr: u64,
    ctor_hash: Vec<(Key, i64)>,
    ops: Vec<Op>,
}

fn rand_int(r: &mut Rng, wide: bool) -> i64 {
    match r.below(if wide { 8 } else { 4 }) {
        0 | 1 => r.below(20) as i64 + 1,
        2 => r.below(200) as i64 - 20,
        3 => r.below(2000) as i64 + 1,
        4 => (1i64 << r.below(40)) + r.below(3) as i64 - 1,
        5 => -(r.below(1 << 20) as i64),
        6 => i64::MAX - r.below(3) as i64,
        _ => i64::MIN + r.below(2) as i64,
    }
}

fn rand_key(r: &mut Rng, shape: u64) -> Key {
    match shape {
        0 => Key::Int(r.below(40) as i64 + 1),
        1 => Key::Int(rand_int(r, true)),
        3 => Key::Str(format!("k{}", r.below(60))),
        6 => match r.below(4) {
            0 => Key::Flt(r.below(100) as f64 / 4.0 + 0.25),
            1 => Key::Bool(r.below(2) == 0),
            2 => Key::Flt(-(r.below(50) as f64) - 0.5),
            _ => Key::Int(r.below(30) as i64),
        },
        _ => match r.below(6) {
            0 | 1 => Key::Int(rand_int(r, shape == 5)),
            2 | 3 => Key::Str(format!("name{}", r.below(40))),
            4 => Key::Str(format!(
                "a long string key to exceed the short limit of forty chars {}",
                r.below(8)
            )),
            _ => Key::Flt(r.below(20) as f64 + 0.5),
        },
    }
}

fn gen_script(r: &mut Rng) -> Script {
    let shape = r.below(8);
    let ctor_arr = if r.below(2) == 0 { 0 } else { r.below(12) };
    let mut ctor_hash = Vec::new();
    if shape >= 2 && r.below(2) == 0 {
        for _ in 0..r.below(8) {
            let k = rand_key(r, 3);
            if !ctor_hash
                .iter()
                .any(|(kk, _): &(Key, i64)| kk.lua() == k.lua())
            {
                ctor_hash.push((k, r.below(1000) as i64));
            }
        }
    }
    let nops = match shape {
        5 => 150 + r.below(200),
        _ => 10 + r.below(60),
    };
    let mut ops = Vec::new();
    for i in 0..nops {
        let op = match r.below(16) {
            0..=7 => {
                let k = match shape {
                    2 => {
                        if r.below(2) == 0 {
                            rand_key(r, 0)
                        } else {
                            rand_key(r, 3)
                        }
                    }
                    4 => rand_key(r, 7),
                    s => rand_key(r, s),
                };
                Op::Set(k, Some(r.below(100000) as i64))
            }
            8 | 9 => Op::Append(r.below(100000) as i64),
            10 => Op::Pop,
            11 => {
                let k = rand_key(r, if shape == 0 { 0 } else { shape });
                Op::Set(k, None)
            }
            12 => {
                if shape == 4 || r.below(4) == 0 {
                    Op::DelTrav(r.below(4) + 2)
                } else {
                    Op::RewriteTrav
                }
            }
            13 => {
                if r.below(4) == 0 {
                    Op::Fill(r.below(64) + 1)
                } else {
                    Op::Append(1)
                }
            }
            _ => Op::Dump,
        };
        ops.push(op);
        if i % 5 == 4 || std::env::var_os("PERSTEP").is_some() {
            ops.push(Op::Dump);
        }
    }
    ops.push(Op::Dump);
    Script {
        ctor_arr,
        ctor_hash,
        ops,
    }
}

// The reference must not collect garbage while the script runs: a GC turns the keys of deleted
// entries into dead keys, which changes node reuse. `collectgarbage("stop")` is not enough by itself
// because e.g. `table.concat`/`gsub` (luaL_Buffer) trigger a full collection internally, so `dump`
// below builds its output with plain concatenation.
const PRELUDE: &str = r#"
collectgarbage("stop")
local function dump(id, tag, t)
  -- NOTE: no table.concat here: luaL_Buffer boxes can trigger a full GC even after collectgarbage("stop"),
  -- and GC-created dead keys change node reuse (what we want to compare is the plain layout).
  local out = ""
  for k, v in pairs(t) do
    local ks
    if math.type(k) == "integer" then ks = "i" .. k
    elseif math.type(k) == "float" then ks = "f" .. string.format("%016x", string.unpack("<I8", string.pack("<d", k)))
    elseif type(k) == "string" then ks = "s" .. k
    else ks = "b" .. tostring(k) end
    out = out .. ks .. "=" .. tostring(v) .. " "
  end
  print(id .. ":" .. tag, #t, out:sub(1, -2))
end
"#;

fn lua_text(id: usize, s: &Script) -> String {
    let mut o = String::new();
    writeln!(o, "do").unwrap();
    write!(o, "local t = {{").unwrap();
    for i in 0..s.ctor_arr {
        write!(o, "{},", 100 + i).unwrap();
    }
    for (k, v) in &s.ctor_hash {
        write!(o, "[{}]={},", k.lua(), v).unwrap();
    }
    writeln!(o, "}}").unwrap();
    let mut tag = 0;
    for op in &s.ops {
        match op {
            Op::Set(k, Some(v)) => writeln!(o, "t[{}]={}", k.lua(), v).unwrap(),
            Op::Set(k, None) => writeln!(o, "t[{}]=nil", k.lua()).unwrap(),
            Op::Append(v) => writeln!(o, "t[#t+1]={v}").unwrap(),
            Op::Pop => writeln!(o, "t[#t]=nil").unwrap(),
            Op::DelTrav(m) => writeln!(
                o,
                "do local c=0 for k in pairs(t) do c=c+1 if c%{m}==0 then t[k]=nil end end end"
            )
            .unwrap(),
            Op::RewriteTrav => {
                writeln!(o, "do local c=0 for k in pairs(t) do c=c+1 t[k]=c end end").unwrap()
            }
            Op::Fill(n) => writeln!(o, "for i=1,{n} do t[i]=i end").unwrap(),
            Op::Dump => {
                writeln!(o, "dump({id}, {tag}, t)").unwrap();
                tag += 1;
            }
        }
    }
    writeln!(o, "end").unwrap();
    o
}

fn to_value(h: &mut Heap, k: &Key) -> Value {
    match k {
        Key::Int(i) => Value::int(*i),
        Key::Flt(f) => Value::float(*f),
        Key::Bool(b) => Value::boolean(*b),
        Key::Str(s) => Value::string(h.new_str(s.as_bytes()).unwrap()),
    }
}

fn dump(h: &Heap, id: usize, tag: usize, t: TableRef) -> String {
    let mut out = Vec::new();
    let mut k = Value::NIL;
    while let Some((nk, nv)) = t.next(&k).unwrap() {
        let ks = if let Some(i) = nk.as_int() {
            format!("i{i}")
        } else if let Some(f) = nk.as_float() {
            format!("f{:016x}", f.to_bits())
        } else if let Some(s) = nk.as_string() {
            format!("s{}", String::from_utf8_lossy(s.as_bytes()))
        } else {
            format!("b{}", nk.as_bool().unwrap())
        };
        out.push(format!("{}={}", ks, nv.as_int().unwrap()));
        k = nk;
    }
    format!("{id}:{tag}\t{}\t{}", t.len(h), out.join(" "))
}

fn run_ours(id: usize, s: &Script) -> Vec<String> {
    let mut h = Heap::new(0).unwrap();
    let t = h.new_table().unwrap();
    // OP_NEWTABLE: exact array size, hash size rounded up to a power of two
    let nh = s.ctor_hash.len() as u32;
    t.resize(
        &mut h,
        s.ctor_arr as u32,
        if nh == 0 { 0 } else { nh.next_power_of_two() },
    )
    .unwrap();
    for (k, v) in &s.ctor_hash {
        let kv = to_value(&mut h, k);
        t.set(&mut h, &kv, &Value::int(*v)).unwrap();
    }
    for i in 0..s.ctor_arr {
        t.set_int(&mut h, i as i64 + 1, &Value::int(100 + i as i64))
            .unwrap();
    }
    let mut lines = Vec::new();
    let mut tag = 0;
    for op in &s.ops {
        match op {
            Op::Set(k, v) => {
                let kv = to_value(&mut h, k);
                let vv = v.map_or(Value::NIL, Value::int);
                t.set(&mut h, &kv, &vv).unwrap();
            }
            Op::Append(v) => {
                let n = t.len(&h);
                t.set_int(&mut h, n as i64 + 1, &Value::int(*v)).unwrap();
            }
            Op::Pop => {
                let n = t.len(&h);
                t.set_int(&mut h, n as i64, &Value::NIL).unwrap();
            }
            Op::DelTrav(m) => {
                let (mut c, mut k) = (0u64, Value::NIL);
                while let Some((nk, _)) = t.next(&k).unwrap() {
                    c += 1;
                    if c % m == 0 {
                        t.set(&mut h, &nk, &Value::NIL).unwrap();
                    }
                    k = nk;
                }
            }
            Op::RewriteTrav => {
                let (mut c, mut k) = (0i64, Value::NIL);
                while let Some((nk, _)) = t.next(&k).unwrap() {
                    c += 1;
                    t.set(&mut h, &nk, &Value::int(c)).unwrap();
                    k = nk;
                }
            }
            Op::Fill(n) => {
                for i in 1..=*n as i64 {
                    t.set_int(&mut h, i, &Value::int(i)).unwrap();
                }
            }
            Op::Dump => {
                lines.push(dump(&h, id, tag, t));
                tag += 1;
            }
        }
    }
    lines
}

#[test]
fn table_order_and_length_match_reference() {
    if cfg!(miri) {
        // Miri cannot spawn the reference: run our side of the first 40 scripts (table code under
        // Miri; scripts/miri.sh). Same scripts as the native run.
        let mut r = Rng(0x00C0_FFEE_1234_5678);
        let mut lines = 0;
        for i in 0..40 {
            lines += run_ours(i, &gen_script(&mut r)).len();
        }
        assert!(lines > 0);
        return;
    }
    let ver = if cfg!(rustynail_lua54) { "54" } else { "55" };
    let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../ref/{ver}/build-seed0/refrun"));
    if !bin.exists() {
        eprintln!(
            "NOTICE: {} missing; skipping table differential",
            bin.display()
        );
        return;
    }
    let mut r = Rng(0x00C0_FFEE_1234_5678);
    let nscripts = 400;
    let scripts: Vec<Script> = (0..nscripts).map(|_| gen_script(&mut r)).collect();
    // One reference process per script (keeps each chunk small and failures easy to isolate).
    let dir = std::env::temp_dir().join(format!("rustynail-tabdiff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("script.lua");
    let mut want: Vec<String> = Vec::new();
    for (i, s) in scripts.iter().enumerate() {
        let text = format!("{PRELUDE}{}", lua_text(i, s));
        std::fs::write(&path, &text).unwrap();
        let out = Command::new(&bin).arg(&path).output().unwrap();
        assert!(
            out.status.success(),
            "refrun failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        want.extend(
            String::from_utf8(out.stdout)
                .unwrap()
                .lines()
                .map(str::to_string),
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    let mut got = Vec::new();
    for (i, s) in scripts.iter().enumerate() {
        got.extend(run_ours(i, s));
    }
    assert_eq!(want.len(), got.len(), "line count");
    let mut bad = 0;
    for (w, g) in want.iter().zip(&got) {
        if w != g {
            bad += 1;
            if bad <= 5 {
                eprintln!("MISMATCH\n ref : {w}\n ours: {g}");
            }
        }
    }
    assert_eq!(bad, 0, "{bad} mismatching dumps out of {}", got.len());
    eprintln!(
        "table differential: {} scripts, {} dumps identical",
        nscripts,
        got.len()
    );
}

#[test]
#[ignore]
fn dump_script() {
    let id: usize = std::env::var("SCRIPT").unwrap().parse().unwrap();
    let mut r = Rng(0x00C0_FFEE_1234_5678);
    let scripts: Vec<Script> = (0..400).map(|_| gen_script(&mut r)).collect();
    println!("{}", PRELUDE);
    println!("{}", lua_text(id, &scripts[id]));
}
