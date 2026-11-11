//! Call string/utf8/table/math library functions with arbitrary arguments (formats, patterns,
//! pack formats, ...) inside the sandbox.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rn_core::value::Value;
use rustynail_fuzz::{new_sandbox, run_budgeted};

const FUNCS: &[(&str, &str)] = &[
    ("string", "format"), ("string", "pack"), ("string", "unpack"), ("string", "packsize"),
    ("string", "find"), ("string", "match"), ("string", "gmatch"), ("string", "gsub"),
    ("string", "rep"), ("string", "byte"), ("string", "char"), ("string", "sub"),
    ("string", "reverse"), ("string", "upper"), ("string", "lower"), ("string", "len"),
    ("utf8", "char"), ("utf8", "codepoint"), ("utf8", "len"), ("utf8", "offset"), ("utf8", "codes"),
    ("table", "concat"), ("table", "unpack"), ("table", "pack"), ("table", "sort"), ("table", "move"),
    ("table", "new"), ("table", "clear"), ("buffer", "encode"), ("buffer", "decode"), ("buffer", "new"),
    ("math", "tointeger"), ("math", "fmod"), ("math", "random"), ("_G", "tostring"), ("_G", "tonumber"),
    ("_G", "select"), ("_G", "load"),
];

fn take<'a>(d: &mut &'a [u8], max: usize) -> &'a [u8] {
    let Some((&n, rest)) = d.split_first() else { return &[] };
    let n = (n as usize % (max + 1)).min(rest.len());
    let (a, b) = rest.split_at(n);
    *d = b;
    a
}

fuzz_target!(|data: &[u8]| {
    let mut d = data;
    let Some((&cfg, rest)) = d.split_first() else { return };
    d = rest;
    let Some((&which, rest)) = d.split_first() else { return };
    d = rest;
    let Some((&shape, rest)) = d.split_first() else { return };
    d = rest;
    let (module, name) = FUNCS[which as usize % FUNCS.len()];
    let Some(mut ls) = new_sandbox(cfg) else { return };
    let (a, b, c) = (take(&mut d, 80), take(&mut d, 80), take(&mut d, 80));
    let n1 = i64::from_le_bytes(std::array::from_fn(|i| *d.get(i).unwrap_or(&0))) % 1000;
    let n2 = i64::from_le_bytes(std::array::from_fn(|i| *d.get(8 + i).unwrap_or(&1))) % 1000;
    let g = ls.globals();
    let Ok(m) = ls.raw_get_str(g, module.as_bytes()) else { return };
    let f = if module == "_G" { Value::table(g) } else { m };
    let Some(t) = f.as_table() else { return };
    let Ok(func) = ls.raw_get_str(t, name.as_bytes()) else { return };
    let Ok(sa) = ls.new_str_value(a) else { return };
    // patterns can backtrack exponentially (in the reference too): keep them short
    let b = if matches!(name, "find" | "match" | "gmatch" | "gsub") { &b[..b.len().min(10)] } else { b };
    let Ok(sb) = ls.new_str_value(b) else { return };
    let Ok(sc) = ls.new_str_value(c) else { return };
    let base = ls.top();
    ls.push(func);
    // argument shapes: strings, strings + integers, integers, mixed
    let args: Vec<Value> = match shape % 5 {
        0 => vec![sa, sb, sc],
        1 => vec![sa, Value::int(n1), Value::int(n2)],
        2 => vec![Value::int(n1), Value::int(n2), sa],
        3 => vec![sa, sb, Value::int(n1), Value::int(n2)],
        _ => vec![sa, Value::float(n1 as f64 / 7.0), sb],
    };
    for v in args {
        ls.push(v);
    }
    let _ = run_budgeted(&mut ls, base, 1);
    ls.collect(false);
});
