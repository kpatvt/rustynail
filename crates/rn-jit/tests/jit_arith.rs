//! J1b end to end: arithmetic / bitwise / comparison / test / upvalue / float-loop templates run inside real
//! Lua programs, compared against the jit-off twin state (the whole binary runs in `always` mode).
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
    });
}

fn run_with(src: &str, mode: Mode, budget: Option<i32>) -> (String, Stats) {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => panic!("snippet must return a string or a number, got {v:?}"),
        }
    } else {
        format!("ERROR {status:?}: {v:?}")
    };
    st.set_top(base);
    (out, rn_jit::stats(&mut st))
}

/// Same result with the JIT off and on (also with small instruction budgets, which force count exits at
/// block heads); returns the stats of the plain jit-on run.
fn same(src: &str) -> Stats {
    let (off, s_off) = run_with(src, Mode::Off, None);
    assert_eq!(s_off.enters, 0);
    let (on, s_on) = run_with(src, Mode::Always, None);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    for b in [3, 17, 200] {
        let (on, _) = run_with(src, Mode::Always, Some(b));
        assert_eq!(off, on, "budget {b}: jit-on result differs for:\n{src}");
    }
    s_on
}

/// Prelude: value list, formatting, and a generator of two-operand functions.
const PRE: &str = r##"
local nan, inf = 0/0, math.huge
local vals = { 0, 1, -1, 2, 3, 7, 63, 64, 100, -100, math.maxinteger, math.mininteger, math.maxinteger - 1,
  1 << 53, (1 << 53) + 1, 0.0, -0.0, 1.0, 2.5, -2.5, 0.5, 1e300, -1e300, nan, inf, -inf, 2.0^63, -2.0^63, 2.0^53,
  nil, true, false, "10", "0x10", "abc", "1e2", " 5 ", {} }
local function fmt(x)
  local mt = math.type(x)
  if mt then return mt .. ":" .. string.format("%.17g", x) end
  if type(x) == "table" then return "table" end
  return tostring(x)
end
local function binop(op)
  return load("local a, b = ... return a " .. op .. " b") and
         load("return function(a, b) return a " .. op .. " b end")()
end
local function table_of(f)
  local out = {}
  for i = 1, 40 do
    for j = 1, 40 do
      local ok, r = pcall(f, vals[i], vals[j])
      out[#out + 1] = (ok and fmt(r) or "E")
    end
  end
  return table.concat(out, ",")
end
"##;

fn prog(body: &str) -> String {
    format!("{PRE}\n{body}")
}

#[test]
fn binary_operators_on_all_value_pairs() {
    // `vals` has a nil hole at the end of #vals: use explicit n via select
    let s = same(&prog(
        r##"
        local h = {}
        for _, op in ipairs{"+", "-", "*", "/", "//", "%", "&", "|", "~", "<<", ">>", "<", "<=", ">", ">=", "==", "~=", "..", "^"} do
          local f = binop(op)
          local out = {}
          for i = 1, 40 do
            for j = 1, 40 do
              local ok, r = pcall(f, vals[i], vals[j])
              out[#out + 1] = (ok and fmt(r) or "E")
            end
          end
          h[#h + 1] = op .. "=" .. table.concat(out, ",")
        end
        return table.concat(h, ";")
    "##,
    ));
    assert!(s.compiled >= 19, "{s:?}");
}

#[test]
fn constant_and_immediate_forms() {
    same(&prog(
        r##"
        local forms = {"a + 1", "a + 100", "a - 3", "a + 1.5", "a - 2.5", "a * 2.5", "a * 7", "a / 2", "a / 0.5",
          "a // 3", "a % 3", "a // -1", "a % -1", "a % -7", "a // 2.5", "a % 2.5", "a // 0", "a % 0", "a & 255",
          "a | 3", "a ~ 5", "a << 3", "a >> 2", "a >> -2", "a << 70", "3 << a", "-1 << a", "~a", "-a", "not a",
          "a < 5", "a <= 5", "a > 5", "a >= 5", "a == 5", "a ~= 5", "a == 2.5", "a < 2.5", "a == nil", "a == true",
          "a == 'abc'", "a == 9007199254740993", "a > 1e100", "a >= -3", "#vals == a", "a ^ 2", "a + 4294967296"}
        local h = {}
        for _, e in ipairs(forms) do
          local f = load("return function(a, vals) return " .. e .. " end")()
          local out = {}
          for i = 1, 40 do
            local ok, r = pcall(f, vals[i], vals)
            out[#out + 1] = (ok and fmt(r) or "E")
          end
          h[#h + 1] = e .. "=" .. table.concat(out, ",")
        end
        return table.concat(h, ";")
    "##,
    ));
}

#[test]
fn logic_and_tests() {
    same(&prog(
        r##"
        local h = {}
        local function f1(a, b) return a and b end
        local function f2(a, b) return a or b end
        local function f3(a, b) if a then return 1 else return 2 end end
        local function f4(a, b) if not a then return 1 else return 2 end end
        local function f5(a, b) local c = a or b return c end
        local function f6(a, b) if a == b then return 1 end return 2 end
        local function f7(a, b) if a < b then return 1 end return 2 end
        local function f8(a, b) while a do a = false end return not b end
        for k, f in ipairs{f1, f2, f3, f4, f5, f6, f7, f8} do
          h[#h + 1] = k .. "=" .. table_of(f)
        end
        return table.concat(h, ";")
    "##,
    ));
}

#[test]
fn metamethods_and_errors_still_work() {
    same(&prog(
        r##"
        local mt = {}
        mt.__add = function(a, b) return "add" end
        mt.__lt = function(a, b) return true end
        mt.__le = function(a, b) return false end
        mt.__eq = function(a, b) return true end
        mt.__unm = function(a) return "unm" end
        mt.__bnot = function(a) return "bnot" end
        mt.__shl = function(a, b) return "shl" end
        mt.__idiv = function(a, b) return "idiv" end
        local x, y = setmetatable({}, mt), setmetatable({}, mt)
        local out = {}
        local function t(f, ...) local ok, r = pcall(f, ...) out[#out + 1] = tostring(ok) .. fmt(r) end
        t(function(a, b) return a + b end, x, 1)
        t(function(a, b) return 1 + b end, x, y)
        t(function(a, b) return a < b end, x, y)
        t(function(a, b) return a <= b end, x, y)
        t(function(a, b) return a == b end, x, y)
        t(function(a, b) return -a end, x)
        t(function(a, b) return ~a end, x)
        t(function(a, b) return a << 1 end, x)
        t(function(a, b) return a // 2 end, x)
        t(function(a, b) return a + 1 end, nil)
        t(function(a, b) return a.x + 1 end, {})
        t(function(a, b) return a // 0 end, 5)
        t(function(a, b) return a % 0 end, 5)
        t(function(a, b) return a & 1 end, 1.5)
        t(function(a, b) return a & 1 end, "3")
        t(function(a, b) return a + 1 end, "10")
        t(function(a, b) return a < b end, 1, "x")
        t(function(a, b) return a < b end, "a", "b")
        return table.concat(out, ";")
    "##,
    ));
}

#[test]
fn error_messages_carry_variable_names() {
    same(&prog(
        r##"
        local out = {}
        local function t(f, ...) local ok, r = pcall(f, ...) out[#out + 1] = tostring(r) end
        local up
        t(function(a) local x = nil return x + 1 end)
        t(function(a) return a + up end, 1)
        t(function(a) return a.b * 2 end, {})
        t(function(a) return -a end, {})
        t(function(a) return a .. 1 end, {})
        t(function(a) return #a + 1 end, nil)
        t(function(a) return a < {} end, 1)
        t(function(a) return math.huge // 0 end)
        t(function(a) return 1 // 0 end)
        t(function(a) return 1 % 0 end)
        t(function(a) return 2^53 | 0 end)
        t(function(a) return 1.5 | 0 end)
        return table.concat(out, ";")
    "##,
    ));
}

#[test]
fn upvalues_get_and_set() {
    same(&prog(
        r##"
        local n, f, s, t = 0, 1.5, "x", {}
        local function inc() n = n + 1 f = f * 2 return n, f end
        local function setall(a, b) n = a f = b s = a t = b return n, f, s, t end
        local function getall() return n, f, s, t end
        local out = {}
        for i = 1, 5 do
          local a, b = inc()
          out[#out + 1] = fmt(a) .. fmt(b)
        end
        local a, b, c, d = setall(3, 4.5)
        out[#out + 1] = fmt(a) .. fmt(b) .. fmt(c) .. fmt(d)
        setall("str", {})
        out[#out + 1] = fmt(n) .. fmt(f) .. tostring(s == "str")
        a, b, c, d = getall()
        out[#out + 1] = fmt(a) .. fmt(b)
        -- an open upvalue (still on the stack) and a closed one
        local function counter()
          local c = 0
          local function bump() c = c + 1 return c end
          bump() bump()
          return bump, function() return c end
        end
        local bump, get = counter()
        bump() bump()
        out[#out + 1] = get()
        collectgarbage()
        out[#out + 1] = get()
        return table.concat(out, ";")
    "##,
    ));
}

#[test]
fn float_for_loops() {
    same(&prog(
        r##"
        local out = {}
        local function loop(a, b, c)
          local s, n = 0.0, 0
          for i = a, b, c do s = s + i n = n + 1 if n > 50 then break end end
          return s, n
        end
        local cases = { {1.0, 3.0, 1}, {1, 3, 0.5}, {1.5, 4, 1}, {3.0, 1.0, -1}, {3, 1, -0.5}, {0.0, 0.0, 1},
          {1.0, 0.0, 1}, {0.1, 1.0, 0.1}, {1, 2, 0.3}, {-0.0, 1, 1}, {1e308, inf, 1e308}, {0, nan, 1}, {nan, 3, 1},
          {1, 3, nan}, {1, 10, 2^53}, {2^53, 2^53 + 4, 1}, {1.0, 2, 0}, {math.maxinteger + 0.0, math.huge, 1},
          {1, 3.5, 1}, {1, 3, 1}, {10, 1, -3}, {-1, -3, -0.5}, {3, 1, 1}, {1, 1, 1.0} }
        for _, c in ipairs(cases) do
          local ok, s, n = pcall(loop, c[1], c[2], c[3])
          out[#out + 1] = ok and (fmt(s) .. "/" .. tostring(n)) or "E:" .. tostring(s)
        end
        -- a long float loop (hot, runs natively) with a nested int loop
        local acc = 0.0
        for x = 0.0, 100.0, 0.25 do
          for k = 1, 10 do acc = acc + x * k end
        end
        out[#out + 1] = fmt(acc)
        for _, c in ipairs{"for i = 1, 'x' do end", "for i = 'a', 2 do end", "for i = 1, 2, {} do end",
                            "for i = 1.5, 2, 0 do end", "for i = 1, 2, 0 do end"} do
          local ok, e = pcall(load(c))
          out[#out + 1] = tostring(e)
        end
        return table.concat(out, ";")
    "##,
    ));
}

#[test]
fn integer_loops_and_overflow() {
    same(&prog(
        r##"
        local out = {}
        local s = 0
        for i = math.maxinteger - 3, math.maxinteger do s = s + i end
        out[#out + 1] = fmt(s)
        s = 1
        for i = 1, 70 do s = s * 3 end
        out[#out + 1] = fmt(s)
        s = math.mininteger
        for i = 1, 5 do s = s - 1 end
        out[#out + 1] = fmt(s)
        local x = 12345
        for i = 1, 100 do x = (x * 1103515245 + 12345) & 0x7fffffff end
        out[#out + 1] = fmt(x)
        local f = 0.0
        for i = 1, 100 do f = f + i / 3 - i // 3 * 0.5 + i % 7 end
        out[#out + 1] = fmt(f)
        local cnt = 0
        for i = 1, 3000 do if i % 3 == 0 and i % 5 ~= 0 then cnt = cnt + 1 elseif i > 2990 then cnt = cnt - 1 end end
        out[#out + 1] = fmt(cnt)
        local w = 0
        while w < 1000 do w = w + 7 end
        out[#out + 1] = fmt(w)
        repeat w = w - 13 until w < 0
        out[#out + 1] = fmt(w)
        return table.concat(out, ";")
    "##,
    ));
}

#[test]
fn hot_loops_run_natively() {
    let src = r##"
        local s, f = 0, 0.0
        for i = 1, 300000 do
          s = s + i * 2 - (i >> 1)
          f = f + s / 3.0
          if s % 7 == 0 then s = s + 1 end
        end
        return string.format("%d %.6g", s, f)
    "##;
    let st = same(src);
    // (the vararg main chunk is entered at pc 0 since VARARGPREP is a template; before J2b it needed OSR)
    assert!(st.osr_enters >= 1 || st.enters >= 1, "{st:?}");
    assert!(
        st.exit_continue < 200,
        "the loop must stay in compiled code: {st:?}"
    );
}

#[test]
fn count_and_line_hooks_with_templates() {
    same(
        r##"
        local n = 0
        local function f(m)
          local s = 0
          for i = 1, m do s = s + i * 2 if s % 3 == 0 then s = s + 1 end end
          return s
        end
        f(100)
        debug.sethook(function() n = n + 1 end, '', 7)
        local r = f(500)
        debug.sethook()
        return n .. ':' .. r
    "##,
    );
    same(
        r##"
        local lines = {}
        local function f(m)
          local s = 0
          for i = 1, m do
            s = s + i
            if s > 10 then
              s = s - 1
            end
          end
          return s
        end
        f(100)
        debug.sethook(function(e, l) lines[#lines + 1] = l end, 'l')
        local r = f(6)
        debug.sethook()
        return table.concat(lines, ',') .. ':' .. r
    "##,
    );
}

#[test]
fn coroutines_and_gc_with_templates() {
    same(&prog(
        r##"
        local co = coroutine.wrap(function(a)
          local s = 0
          for i = 1, 20 do s = s + a * i a = coroutine.yield(s) or a end
          return s
        end)
        local out = {}
        for i = 1, 21 do out[#out + 1] = fmt(co(i)) end
        collectgarbage()
        local t = {}
        for i = 1, 200 do t[i] = i * 1.5 + #out end
        collectgarbage("step", 100)
        return table.concat(out, ",") .. #t
    "##,
    ));
}
