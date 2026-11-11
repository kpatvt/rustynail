//! End-to-end tests of the State layer with the *real compiler*: Lua text -> `ProtoData` -> `Proto`
//! -> closure -> `call` through the real interpreter (`rn_core::vm`), with a handful of native
//! stand-ins for the base library. Outputs are compared with the reference interpreter
//! (`ref/55/build/lua`) when it is available.
//!
//! The same scripts also run under GC stress (a full collection at every safepoint) and the final
//! state is checked for leaks by `Heap`'s drop assertion.
#![cfg(not(rustynail_lua54))]
use rn_core::error::chunk_id;
use rn_core::meta::type_name;
use rn_core::number::{self, Number};
use rn_core::state::{Ci, LuaState, NativeFn, State, StkId, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn emit(b: &[u8]) {
    OUT.with(|o| o.borrow_mut().extend_from_slice(b));
}

// ---- native stand-ins for the base library --------------------------------------------------

fn tostr(v: &Value) -> Vec<u8> {
    if let Some(s) = v.as_string() {
        return s.as_bytes().to_vec();
    }
    if let Some(n) = v.as_number() {
        return number::tostring(n).as_bytes().to_vec();
    }
    if v.is_nil() {
        return b"nil".to_vec();
    }
    if let Some(b) = v.as_bool() {
        return if b {
            b"true".to_vec()
        } else {
            b"false".to_vec()
        };
    }
    format!("{}: 0x0", String::from_utf8_lossy(type_name(v.type_tag()))).into_bytes()
}

fn n_print(st: &mut State) -> LuaResult<usize> {
    let line: Vec<Vec<u8>> = st.args().iter().map(tostr).collect();
    emit(&line.join(&b'\t'));
    emit(b"\n");
    Ok(0)
}

fn n_type(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let s = st.new_str_value(type_name(v.type_tag()))?;
    st.push(s);
    Ok(1)
}

fn n_tostring(st: &mut State) -> LuaResult<usize> {
    let b = tostr(&st.arg(0));
    let s = st.new_str_value(&b)?;
    st.push(s);
    Ok(1)
}

fn n_setmetatable(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    let mt = st.arg(1).as_table();
    st.set_metatable(&t, mt);
    st.push(t);
    Ok(1)
}

fn n_getmetatable(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    let v = st.metatable_of(&t).map_or(Value::NIL, Value::table);
    st.push(v);
    Ok(1)
}

fn n_rawequal(st: &mut State) -> LuaResult<usize> {
    let r = st.arg(0).raw_equal(&st.arg(1));
    st.push(Value::boolean(r));
    Ok(1)
}

fn where_(st: &State, level: usize) -> Vec<u8> {
    let mut ci = st.ci();
    for _ in 0..level {
        match ci.previous() {
            Some(p) => ci = p,
            None => return Vec::new(),
        }
    }
    match st.ci_lclosure(ci) {
        Some(cl) => {
            let line = st.current_line(ci);
            let mut out = match cl.proto().source() {
                Some(s) => chunk_id(s.get().as_bytes()),
                None => b"?".to_vec(),
            };
            out.extend_from_slice(format!(":{line}: ").as_bytes());
            out
        }
        None => Vec::new(),
    }
}

fn n_error(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0);
    let level = st.arg(1).as_int().unwrap_or(1);
    if v.is_string() && level > 0 {
        let mut m = where_(st, level as usize);
        m.extend_from_slice(v.as_string().unwrap().as_bytes());
        let s = st.new_str_value(&m)?;
        return Err(st.throw_value(s));
    }
    Err(st.throw_value(v))
}

fn n_assert(st: &mut State) -> LuaResult<usize> {
    if st.arg(0).is_falsy() {
        let m = if st.nargs() > 1 {
            st.arg(1)
        } else {
            st.new_str_value(b"assertion failed!")?
        };
        return Err(st.throw_value(m));
    }
    Ok(st.nargs())
}

fn n_select(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let a = st.arg(0);
    if a.as_string().is_some_and(|s| s.as_bytes() == b"#") {
        st.push(Value::int(n as i64 - 1));
        return Ok(1);
    }
    let i = a.as_int().unwrap_or(1);
    let extra = n - 1;
    let cnt = if i < 0 {
        let k = i.unsigned_abs() as usize;
        if k > extra {
            let m = st.new_str_value(b"bad argument #1 to 'select' (index out of range)")?;
            return Err(st.throw_value(m));
        }
        k
    } else if i as usize > extra {
        0
    } else {
        extra - i as usize + 1
    };
    Ok(cnt)
}

fn n_pcall(st: &mut State) -> LuaResult<usize> {
    let base = st.ci().r().func + 1;
    let top = st.top();
    st.check_stack(1)?;
    // shift arguments up and insert `true` at the bottom
    let mut p = top;
    while p > base {
        let v = st.get(p - 1);
        st.set(p, v);
        p -= 1;
    }
    st.set(base, Value::TRUE);
    st.set_top(top + 1);
    let status = st.pcall(base + 1, LUA_MULTRET, 0);
    if status != Status::Ok {
        st.set(base, Value::FALSE);
    }
    Ok(st.top() - base)
}

fn n_collect(st: &mut State) -> LuaResult<usize> {
    st.collect(false);
    Ok(0)
}

fn n_next(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0).as_table().unwrap();
    match t.next(&st.arg(1)) {
        Ok(Some((k, v))) => {
            st.push(k);
            st.push(v);
            Ok(2)
        }
        Ok(None) => {
            st.push(Value::NIL);
            Ok(1)
        }
        Err(e) => Err(st.table_error(e)),
    }
}

fn n_pairs(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    st.push(Value::native(n_next));
    st.push(t);
    st.push(Value::NIL);
    Ok(3)
}

fn n_ipairs_iter(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0).as_table().unwrap();
    let i = st.arg(1).as_int().unwrap() + 1;
    let v = t.get_int(i);
    if v.is_nil() {
        st.push(Value::NIL);
        Ok(1)
    } else {
        st.push(Value::int(i));
        st.push(v);
        Ok(2)
    }
}

fn n_ipairs(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0);
    st.push(Value::native(n_ipairs_iter));
    st.push(t);
    st.push(Value::int(0));
    Ok(3)
}

fn n_getn(st: &mut State) -> LuaResult<usize> {
    let t = st.arg(0).as_table().unwrap();
    let n = t.len(st.heap());
    st.push(Value::int(n as i64));
    Ok(1)
}

fn new_test_state(stress: bool) -> LuaState {
    let mut st = rn_core_state();
    let g = st.globals();
    let natives: [(&str, NativeFn); 15] = [
        ("print", n_print),
        ("type", n_type),
        ("tostring", n_tostring),
        ("setmetatable", n_setmetatable),
        ("getmetatable", n_getmetatable),
        ("rawequal", n_rawequal),
        ("error", n_error),
        ("assert", n_assert),
        ("select", n_select),
        ("pcall", n_pcall),
        ("collectgarbage", n_collect),
        ("next", n_next),
        ("pairs", n_pairs),
        ("ipairs", n_ipairs),
        ("rawlen", n_getn),
    ];
    for (name, f) in natives {
        let k = st.new_str_value(name.as_bytes()).unwrap();
        st.raw_set(g, &k, &Value::native(f)).unwrap();
    }
    if stress {
        st.heap().set_gc_stress(true);
    }
    st
}

fn rn_core_state() -> LuaState {
    rustynail::new_state(0).expect("state")
}

/// Compile `src` as chunk `@t.lua`, run it, return everything it printed (or the error).
fn run_script(src: &str, stress: bool) -> String {
    OUT.with(|o| o.borrow_mut().clear());
    let mut st = new_test_state(stress);
    let top = st.top();
    let status = st.load_chunk(src.as_bytes(), b"@t.lua", None);
    if status != Status::Ok {
        let m = String::from_utf8_lossy(st.get(top).as_string().unwrap().as_bytes()).into_owned();
        return format!("LOAD ERROR: {m}");
    }
    let status = st.pcall(top, 0, 0);
    let errmsg =
        (status != Status::Ok).then(|| String::from_utf8_lossy(&tostr(&st.get(top))).into_owned());
    st.set_top(top);
    st.collect(false);
    drop(st); // closing the state runs the remaining finalizers (they may print)
    let mut out = OUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned());
    if let Some(e) = errmsg {
        out.push_str(&format!("ERROR: {e}\n"));
    }
    out
}

fn ref_lua() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ref/55/build/lua");
    p.exists().then_some(p)
}

fn run_reference(src: &str, name: &str) -> Option<String> {
    let lua = ref_lua()?;
    let dir = std::env::temp_dir().join(format!("rustynail-e2e-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("t.lua"), src).ok()?;
    let out = Command::new(lua)
        .arg("t.lua")
        .current_dir(&dir)
        .output()
        .ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        // "lua: t.lua:3: message" + traceback on stderr
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("");
        let first = first.find("t.lua").map_or(first, |i| &first[i..]);
        s.push_str(&format!("ERROR: {first}\n"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    Some(s)
}

/// Run `src` with and without GC stress and compare with the reference interpreter.
fn check(name: &str, src: &str) {
    let plain = run_script(src, false);
    let stressed = run_script(src, true);
    assert_eq!(plain, stressed, "{name}: GC stress changed the result");
    match run_reference(src, name) {
        Some(want) => assert_eq!(
            plain, want,
            "{name}: differs from the reference interpreter"
        ),
        None => eprintln!("note: reference lua not found; {name} not compared"),
    }
}

#[allow(dead_code)]
fn unused(_: Ci, _: StkId, _: Number) {}

// ---------------------------------------------------------------------------------------------

#[test]
fn arithmetic_and_control_flow() {
    check(
        "arith",
        r#"
local a, b = 7, 2
print(a + b, a - b, a * b, a / b, a // b, a % b, a ^ b)
print(-a, 7.5 // 2, -7 // 2, 7 % -3, 2^0.5)
print(1 == 1.0, 1 < 2, 2 <= 2, "a" < "b", not nil, not 0)
local s = 0
for i = 1, 10 do s = s + i end
for i = 10, 1, -3 do s = s + i end
for x = 0.5, 2.5 do s = s + x end
print(s)
local n = 0
while n < 5 do n = n + 1 if n == 3 then goto cont end print("n", n) ::cont:: end
repeat n = n - 1 until n < 2
print(n, "x" .. 1 .. 2.5)
print(#"hello", #{10, 20, 30})
print(10 // 0.0, -10 % 3.0, 3 | 0 == 3)
"#,
    );
}

#[test]
fn closures_and_upvalues() {
    check(
        "closures",
        r#"
local function counter()
  local n = 0
  return function() n = n + 1 return n end, function() return n end
end
local inc, get = counter()
inc() inc()
print(get(), inc(), get())
local fs = {}
for i = 1, 3 do local j = i fs[i] = function() j = j + 10 return j end end
print(fs[1](), fs[1](), fs[2](), fs[3]())
local x = 1
local function f() return x end
x = 2
print(f())
local function outer()
  local a = 1
  local function mid()
    local function inner() a = a + 1 return a end
    return inner
  end
  return mid(), function() return a end
end
local i2, g2 = outer()
i2() i2()
print(g2())
-- loops create fresh locals per iteration
local t = {}
local k = 1
while k <= 3 do local j = k t[k] = function() j = j * 2 return j end k = k + 1 end
print(t[1](), t[1](), t[2](), t[3]())
"#,
    );
}

#[test]
fn varargs_and_multiple_results() {
    check(
        "varargs",
        r#"
local function f(...) return ... end
print(f(1, 2, 3))
print((f(1, 2, 3)))
print(select('#', f()))
print(select('#', f(nil, nil)))
local function g(a, ...) local b, c = ... return a, b, c, select('#', ...) end
print(g(1))
print(g(1, 2, 3, 4))
local function h(...) local t = {...} return #t, ... end
print(h(5, 6, 7))
local function tail(n, ...) if n == 0 then return ... end return tail(n - 1, n, ...) end
print(tail(5))
print(select(-1, 1, 2, 3))
print(select(2, "a", "b", "c"))
-- named vararg table (5.5)
local function named(...t) return t.n, t[1], t[2], ... end
print(named(10, 20, 30))
local function named0(a, ...t) return a, t.n end
print(named0(1), named0(1, 2, 3))
"#,
    );
}

#[test]
fn metamethods() {
    check(
        "metamethods",
        r#"
local mt = {}
mt.__add = function(a, b) return "add" end
mt.__concat = function(a, b) return "cat" end
mt.__eq = function(a, b) return true end
mt.__lt = function(a, b) return true end
mt.__le = function(a, b) return false end
mt.__call = function(self, x, y) return "called", x, y end
mt.__len = function() return 42 end
mt.__unm = function() return "unm" end
mt.__index = function(t, k) return "idx:" .. tostring(k) end
local store = {}
mt.__newindex = function(t, k, v) store[k] = v end
local a, b = setmetatable({}, mt), setmetatable({}, mt)
print(a + 1, 1 + a, a .. "x", "x" .. a, a == b, a < b, a <= b, a > b)
print(a(1, 2), #a, -a, a.foo, a[1])
a.z = 5 print(store.z, rawequal(a, b))
local t = setmetatable({}, {__index = {inherited = "yes"}})
print(t.inherited, t.missing)
local chain = setmetatable({}, {__index = setmetatable({}, {__index = function(_, k) return k .. "!" end})})
print(chain.deep)
-- __call chains
local c1 = setmetatable({}, {__call = function(self, ...) return select('#', ...), ... end})
local c2 = setmetatable({}, {__call = c1})
print((select('#', c2("a", "b"))))
-- errors in metamethods propagate with position info
local bad = setmetatable({}, {__add = function() error("in add") end})
print(pcall(function() return bad + 1 end))
print(pcall(function() return {} + 1 end))
print(pcall(function() return 1 < {} end))
print(pcall(function() return {} < {} end))
print(pcall(function() return "a" < 1 end))
print(pcall(function() return #5 end))
print(pcall(function() return {} .. "x" end))
"#,
    );
}

#[test]
fn errors_pcall_and_positions() {
    check(
        "errors",
        r#"
print(pcall(error, "msg"))
print(pcall(error, "msg", 0))
print(pcall(error))
print(pcall(error, nil))
print(pcall(error, {1}) == false)
local function thrower() error("boom") end
print(pcall(thrower))
local function lvl2() error("lvl2", 2) end
local function caller() lvl2() end
print(pcall(caller))
print(select('#', pcall(function() return 1, 2, 3 end)))
print(pcall(pcall, error, "x"))
local ok, e = pcall(function() local t = nil; return t.x end)
print(ok, type(e))
print(pcall(function() return 1 // 0 end))
print(pcall(function() return 1 % 0 end))
print(pcall(setmetatable({}, {__call = function() error("callmm", 0) end})))
local depth = 0
local function rec() depth = depth + 1 return 1 + rec() end
local ok2, e2 = pcall(rec)
print(ok2, e2, depth > 1000)
local function pc() return pcall(pc) end
local r = {pc()}
print(r[1], #r > 1)
"#,
    );
}

#[test]
fn to_be_closed_variables() {
    check(
        "tbc",
        r#"
local function closer(name, fail)
  return setmetatable({}, {__close = function(_, err)
    print("close", name, err)
    if fail then error("fail in " .. name, 0) end
  end})
end
do
  local a <close> = closer("a")
  local b <close> = closer("b")
  local c <const> = 5
  print("body", c)
end
print(pcall(function()
  local x <close> = closer("x")
  local y <close> = closer("y", true)
  local z <close> = closer("z")
  error("body error", 0)
end))
local function ret()
  local r <close> = closer("r")
  return 1, 2, 3
end
print(ret())
for i = 1, 2 do
  local l <close> = closer("loop" .. i)
  if i == 1 then goto continue end
  print("i", i)
  ::continue::
end
print(pcall(function() local bad <close> = {} end))
local nilclose <close> = nil
local falseclose <close> = false
-- generic for with a closing value
local function iter()
  return function(s, c) if c < 2 then return c + 1 end end, nil, 0, closer("forclose")
end
for i in iter() do print("it", i) end
"#,
    );
}

#[test]
fn tables_pairs_and_gc() {
    check(
        "tables",
        r#"
local t = {1, 2, 3, x = 1, y = 2}
local keys = 0
for k, v in pairs(t) do keys = keys + 1 end
print(keys, #t)
for i, v in ipairs({"a", "b", "c"}) do print(i, v) end
local big = {}
for i = 1, 200 do big[i] = {i, tostring(i)} end
collectgarbage()
local sum = 0
for i = 1, 200 do sum = sum + big[i][1] + #big[i][2] end
print(sum)
local weak = setmetatable({}, {__mode = "v"})
weak[1] = {}
collectgarbage()
print(weak[1])
local gced = {}
for i = 1, 3 do setmetatable({}, {__gc = function() gced[#gced + 1] = i end}) end
collectgarbage()
table_sum = 0 for _, v in ipairs(gced) do table_sum = table_sum + v end
print(#gced, table_sum)
local obj = setmetatable({}, {__gc = function(o) print("closing finalizer") end})
"#,
    );
}

#[test]
fn recursion_and_calls() {
    check(
        "recursion",
        r#"
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
print(fib(18))
local function sum(n, acc) if n == 0 then return acc end return sum(n - 1, acc + n) end
print(sum(100000, 0))   -- tail calls do not grow the stack
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
print(deep(10000))
local t = {}
function t.method(self, x) return self.v + x end
t.v = 10
function t:m2(x) return self:method(x) * 2 end
print(t:m2(5))
print((function(...) return select('#', ...) end)(nil, nil, nil))
print(pcall(deep, 1e7))
"#,
    );
}

#[test]
fn chunk_loading_errors_and_modes() {
    let mut st = new_test_state(false);
    let top = st.top();
    let status = st.load_chunk(b"x = = 1", b"=(load)", None);
    assert_eq!(status, Status::ErrSyntax);
    assert_eq!(
        st.get(top).as_string().unwrap().as_bytes(),
        b"(load):1: unexpected symbol near '='"
    );
    st.set_top(top);
    // mode restrictions
    assert_eq!(
        st.load_chunk(b"return 1", b"=m", Some(b"b")),
        Status::ErrSyntax
    );
    assert_eq!(
        st.get(top).as_string().unwrap().as_bytes(),
        b"attempt to load a text chunk (mode is 'b')"
    );
    st.set_top(top);
    // a loaded chunk's first upvalue is the globals table; setting it changes globals
    assert_eq!(
        st.load_chunk(b"gvar = 123 return gvar", b"=g", None),
        Status::Ok
    );
    assert_eq!(st.pcall(top, 1, 0), Status::Ok);
    assert_eq!(st.get(top).as_int(), Some(123));
    let k = st.new_str_value(b"gvar").unwrap();
    assert_eq!(st.globals().get(&k).as_int(), Some(123));
    st.set_top(top);
}

#[test]
fn stack_usage_is_sane() {
    // deep recursion ends in a catchable "stack overflow" and the stack shrinks back afterwards
    let mut st = new_test_state(false);
    let top = st.top();
    let src = b"local function f(n) return 1 + f(n + 1) end return f(1)";
    assert_eq!(st.load_chunk(src, b"=so", None), Status::Ok);
    assert_eq!(st.pcall(top, 0, 0), Status::ErrRun);
    let m = String::from_utf8_lossy(st.get(top).as_string().unwrap().as_bytes()).into_owned();
    assert!(m.contains("stack overflow"), "{m}");
    st.set_top(top);
    st.shrink_stack();
    assert!(st.stack_size() < 100_000, "stack size {}", st.stack_size());
    assert_eq!(st.ci(), st.base_ci());
}
