//! `string.buffer` and `table.new` / `table.clear` driven by arbitrary operation bytes (as Lua source, so the library
//! natives run exactly as scripts call them) inside the sandbox: buffer ops (put/get/skip/set/reset/free/len/tostring),
//! encode/decode of generated values and of arbitrary bytes, dict / metatable options, round-trip checks. A failed
//! `assert` inside the script, a panic or a sanitizer report is a bug; ordinary Lua errors (bad arguments, malformed
//! input) are expected and ignored.
#![no_main]
use libfuzzer_sys::fuzz_target;
use rustynail_fuzz::{new_sandbox, run_budgeted};
use std::fmt::Write;

fn lit(d: &[u8]) -> String {
    let mut s = String::from("\"");
    for b in d {
        let _ = write!(s, "\\{b:03}");
    }
    s.push('"');
    s
}

fuzz_target!(|data: &[u8]| {
    let Some((&cfg, rest)) = data.split_first() else { return };
    if rest.len() > 600 {
        return;
    }
    let Some(mut ls) = new_sandbox(cfg) else { return };
    let mut src = String::from(
        "local buf, rt = buffer, 0\n\
         local function same(a, b, d) if type(a) ~= type(b) then return false end\n\
           if type(a) ~= 'table' then return a == b or (a ~= a and b ~= b) end\n\
           if d > 20 then return true end\n\
           for k, v in pairs(a) do if not same(v, b[k], d + 1) then return false end end\n\
           for k in pairs(b) do if a[k] == nil then return false end end return true end\n\
         local b = buf.new({ dict = { 'a', 'bb', 'k1' } })\n\
         local t = table.new(4, 4)\n\
         local vals = {}\n",
    );
    let mut i = 0;
    while i < rest.len() {
        let op = rest[i] % 14;
        let n = *rest.get(i + 1).unwrap_or(&0);
        let take = (n as usize % 24).min(rest.len().saturating_sub(i + 2));
        let bytes = &rest[(i + 2).min(rest.len())..(i + 2 + take).min(rest.len())];
        i += 2 + take;
        let l = lit(bytes);
        let _ = match op {
            0 => writeln!(src, "pcall(b.put, b, {l}, {n})"),
            1 => writeln!(src, "pcall(b.get, b, {n} % 7, {n} % 3)"),
            2 => writeln!(src, "pcall(b.skip, b, {n} % 11)"),
            3 => writeln!(src, "pcall(b.set, b, {l})"),
            4 => writeln!(src, "if {n} % 5 == 0 then b:reset() elseif {n} % 5 == 1 then b:free() end"),
            5 => writeln!(src, "assert(#b >= 0 and #tostring(b) == #b)"),
            // decode arbitrary bytes (a standalone string and through the buffer object)
            6 => writeln!(src, "pcall(buf.decode, {l})"),
            7 => writeln!(src, "do local x = buf.new() x:put({l}) pcall(x.decode, x) pcall(x.decode, x) end"),
            // build a value and check the round trip
            8 => writeln!(
                src,
                "do local v = {{ {l}, {n}, {n} / 8, x = {{ {l} }}, [{n} + 0.5] = true }} vals[#vals + 1] = v local s = buf.encode(v) assert(same(buf.decode(s), v, 0)) end"
            ),
            9 => writeln!(
                src,
                "do local v = {{ a = {l}, bb = {n}, k1 = {{ {n} }}, other = false }} local x = buf.new({{ dict = {{ 'a', 'bb', 'k1' }} }}) x:encode(v) assert(same(x:decode(), v, 0)) end"
            ),
            10 => writeln!(src, "pcall(b.encode, b, vals[{n} % (#vals + 1) + 1] or {l})"),
            11 => writeln!(src, "pcall(b.decode, b)"),
            12 => writeln!(src, "do local c = pcall(table.new, {n} % 40, {n} % 9) t = table.new({n} % 40, {n} % 9) t[{n} % 5 + 1] = {l} t[{l}] = {n} end"),
            _ => writeln!(src, "table.clear(t) assert(next(t) == nil) t[1] = {n} t.x = {l}"),
        };
    }
    if ls.load_chunk(src.as_bytes(), b"=fuzz", Some(b"t")) == rn_core::status::Status::Ok {
        let f = ls.top() - 1;
        let _ = run_budgeted(&mut ls, f, 0);
    }
    ls.collect(false);
});
