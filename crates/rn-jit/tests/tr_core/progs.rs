//! The twin programs of `tr_core.rs` (docs/JIT_TRACE.md 9.3 subset). Each program is a chunk that prints its results
//! (the harness captures `print` / `io.write`), keeps every loop inside a function (the main-chunk loop around a call
//! is `InnerLoop` for the prototype recorder) and is deterministic. `small` marks the ones that are cheap enough for
//! the GC-stress runs (loops of at most a few hundred iterations per call, few calls).

/// One program: name, `small`, source.
pub struct Prog {
    pub name: &'static str,
    pub small: bool,
    pub src: &'static str,
}

pub const PROGS: &[Prog] = &[
    Prog {
        name: "int_sum",
        small: true,
        src: r#"
local function f(n) local s = 0 for i = 1, n do s = s + i end return s end
local r = 0
for k = 1, 40 do r = r + f(100 + k) end
print(r, f(1000), f(0), f(1))
"#,
    },
    Prog {
        name: "float_poly",
        small: true,
        src: r#"
local function f(x, n)
  local s = 0.0
  for i = 1, n do s = s * 0.5 + x * x - 3.25 * x + i end
  return s
end
local r = 0.0
for k = 1, 40 do r = r + f(k * 0.25, 90) end
print(string.format("%.9g", r))
"#,
    },
    Prog {
        name: "special_floats",
        small: true,
        src: r#"
local vals = {0.0, -0.0, 1/0, -1/0, 0/0, 2^53, -2^53, 2^63, 1e308, 5e-324, 0.1}
local function f(a, b)
  local t = {}
  for i = 1, 40 do
    local x = a + i - i
    t[#t + 1] = (x + b) * 1.0
  end
  return t[40]
end
local function g(a, b, n)
  local s = 0
  for i = 1, n do
    s = a * b - a / b + (a // 1) + (a % 3.5) - -a
  end
  return s
end
for i = 1, #vals do
  for j = 1, #vals do
    local r1 = g(vals[i], vals[j], 8)
    local r2 = g(vals[i], vals[j], 400)
    print(i, j, string.format("%a %a", r1, r2))
  end
end
"#,
    },
    Prog {
        name: "int_edges",
        small: true,
        src: r#"
local mi, ma = math.mininteger, math.maxinteger
local function g(a, b, n)
  local s = 0
  for i = 1, n do
    s = (a + b) ~ (a - b) | (a & b) + ((a << (i & 63)) >> 3) - (a * b)
    s = s + (b ~= 0 and (a // b + a % b) or 0)
  end
  return s
end
local vs = {0, 1, -1, 2, -2, 7, -7, 63, 64, mi, ma, mi + 1, ma - 1, 1 << 31, 1 << 32}
for i = 1, #vs do
  for j = 1, #vs do
    print(i, j, g(vs[i], vs[j], 300))
  end
end
"#,
    },
    Prog {
        name: "array_rec",
        small: false,
        src: r#"
local function run(t, n, c, x)
  for i = 2, n do t[i] = t[i - 1] * c + x end
  return t[n]
end
local t = {}
for i = 1, 500 do t[i] = 1.0 end
local r = 0.0
for k = 1, 30 do t[1] = k + 0.5; r = r + run(t, 500, 0.5, 0.25) end
print(string.format("%.9g", r), string.format("%.9g", t[400]))
"#,
    },
    Prog {
        name: "int_rec2",
        small: false,
        src: r#"
local function run(t, n)
  for i = 3, n do t[i] = (t[i - 1] + t[i - 2]) & 0xFFFFFF end
  return t[n]
end
local t = {}
for i = 1, 600 do t[i] = i end
local r = 0
for k = 1, 30 do t[1] = k; t[2] = k + 1; r = r + run(t, 600) end
print(r, t[600], t[301])
"#,
    },
    Prog {
        name: "alias_arrays",
        small: true,
        src: r#"
local function f(a, b, n)
  for i = 1, n do
    a[i] = b[i] + 1
    local x = b[i]
    a[i] = x + a[i]
  end
end
local function g(a, b, n)
  local s = 0
  for i = 1, n do a[i] = i; s = s + b[i] end
  return s
end
local a, b = {}, {}
for i = 1, 200 do a[i] = i; b[i] = i * 2 end
for k = 1, 20 do
  f(a, b, 200)
  f(a, a, 200)
  print(k, g(a, b, 200), g(a, a, 200), a[200], b[200])
end
"#,
    },
    Prog {
        name: "holes_oob",
        small: true,
        src: r#"
local function sum(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
for k = 1, 30 do print(sum(t, 100)) end
t[50] = nil
print(pcall(sum, t, 100))
for k = 1, 30 do print(pcall(sum, t, 49)) end
t[50] = 50
print(pcall(sum, t, 101))
local u = {}
for i = 1, 100 do u[i] = i + 0.5 end
for k = 1, 30 do print(sum(u, 100)) end
print(pcall(sum, u, 120))
"#,
    },
    Prog {
        name: "inline_leaf",
        small: true,
        src: r#"
local function leaf(a, b) local t = a * b; return t + a - b end
local function run(n)
  local s = 0.0
  for i = 1, n do s = s + leaf(i + 0.0, 0.5) end
  return s
end
local r = 0.0
for k = 1, 40 do r = r + run(100) end
print(string.format("%.9g", r))
"#,
    },
    Prog {
        name: "inline_2",
        small: true,
        src: r#"
local function leaf(a, b) local t = a * b; return t + a - b end
local function mid(x) local y = leaf(x, 0.5); return y * 0.25 + 1.0 end
local function run(n)
  local s = 0.0
  for i = 1, n do s = s + mid(i + 0.0) end
  return s
end
local r = 0.0
for k = 1, 40 do r = r + run(100) end
print(string.format("%.9g", r))
"#,
    },
    Prog {
        name: "inline_depth",
        small: true,
        src: r#"
local function d5(x) return x + 1 end
local function d4(x) return d5(x) + 1 end
local function d3(x) return d4(x) + 1 end
local function d2(x) return d3(x) + 1 end
local function d1(x) return d2(x) + 1 end
local function run(n)
  local s = 0
  for i = 1, n do s = s + d1(i) + d3(i) end
  return s
end
local function rec(n) if n == 0 then return 0 end return 1 + rec(n - 1) end
local function run2(n)
  local s = 0
  for i = 1, n do s = s + rec(3) end
  return s
end
local r = 0
for k = 1, 30 do r = r + run(60) + run2(60) end
print(r)
"#,
    },
    Prog {
        name: "callee_change",
        small: true,
        src: r#"
local f = function(x) return x + 1 end
local function run(n, k)
  local s = 0
  for i = 1, n do
    if i == k then f = function(x) return x * 2 end end
    s = s + f(i)
  end
  return s
end
for k = 1, 8 do
  f = function(x) return x + 1 end
  print(run(100, k * 11))
end
-- to a C function and to a callable table
local function run2(n, k)
  local s = 0
  for i = 1, n do
    if i == k then f = math.abs end
    s = s + f(-i)
  end
  return s
end
for k = 1, 5 do f = function(x) return -x end; print(run2(100, 20 * k)) end
local function run3(n, k)
  local s = 0
  for i = 1, n do
    if i == k then f = setmetatable({}, {__call = function(_, x) return x * 3 end}) end
    s = s + f(i)
  end
  return s
end
for k = 1, 5 do f = function(x) return x end; print(run3(100, 15 * k)) end
"#,
    },
    Prog {
        name: "upval_acc",
        small: true,
        src: r#"
local acc = 0
local function run(t, n)
  for i = 1, n do acc = acc + t[i] end
end
local t = {}
for i = 1, 200 do t[i] = i end
for k = 1, 40 do run(t, 200) end
print(acc)
local fl = 0.0
local function runf(t, n) for i = 1, n do fl = fl + t[i] * 0.5 end end
local tf = {}
for i = 1, 200 do tf[i] = i + 0.25 end
for k = 1, 40 do runf(tf, 200) end
print(fl)
"#,
    },
    Prog {
        name: "upval_shared",
        small: true,
        src: r#"
local function mk()
  local c = 0
  return function(n) for i = 1, n do c = c + i end return c end,
         function() return c end
end
local inc1, get1 = mk()
local inc2, get2 = mk()
for k = 1, 30 do inc1(100); inc2(50) end
print(get1(), get2())
local function bump(x) return x + 1 end
local u = 0
local function run(n) for i = 1, n do u = u + bump(i) end end
for k = 1, 30 do run(100) end
print(u)
local a, b = 1, 100
local function f1() return a end
local function f2() return b end
local function loop(n) local s = 0 for i = 1, n do s = s + a end return s end
for k = 1, 10 do print(loop(100)) end
debug.upvaluejoin(loop, 1, f2, 1)
for k = 1, 10 do print(loop(100)) end
"#,
    },
    Prog {
        name: "upval_closed_open",
        small: true,
        src: r#"
local function mk()
  local x = 10
  local function run(n)
    local s = 0
    for i = 1, n do x = x + 1; s = s + x end
    return s
  end
  return run, function() return x end
end
local run, get = mk()
for k = 1, 30 do run(100) end
print(get())
local function outer()
  local y = 0
  local function f(n) for i = 1, n do y = y + i end end
  for k = 1, 30 do f(100) end
  return y
end
print(outer())
"#,
    },
    Prog {
        name: "intrinsics",
        small: true,
        src: r#"
local sqrt, abs, floor, ceil, min, max = math.sqrt, math.abs, math.floor, math.ceil, math.min, math.max
local function run(t, n, a, b, c)
  local s = 0.0
  for i = 1, n do
    local x = t[i]
    s = s + sqrt(a * a + b * b) / c + abs(x) + floor(x) + ceil(x) + min(x, a) + max(x, b)
  end
  return s
end
local t = {}
for i = 1, 100 do t[i] = (i - 50) * 0.37 end
local r = 0.0
for k = 1, 30 do r = r + run(t, 100, 3.0 + k, 4.0, 2.0) end
print(string.format("%.9g", r))
local function ints(n)
  local s = 0
  for i = 1, n do s = s + abs(-i) + floor(i) + ceil(i) + min(i, 50) + max(i, 20) end
  return s
end
for k = 1, 30 do r = ints(100) end
print(r)
"#,
    },
    Prog {
        name: "type_entry",
        small: false,
        src: r#"
local function run(t, n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local m = 50
local t = {}
for j = 1, m do t[j] = (j % 9) * 0.25 end
local total = 0.0
for r = 1, 40 do total = total + run(t, 20, m) end
print(string.format("%.6f", total))
"#,
    },
    Prog {
        name: "type_entry_stable",
        small: false,
        src: r#"
local function run(t, n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0.0
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local m = 50
local t = {}
for j = 1, m do t[j] = (j % 9) * 0.25 end
local total = 0.0
for r = 1, 40 do total = total + run(t, 20, m) end
print(string.format("%.6f", total))
"#,
    },
    Prog {
        name: "type_late",
        small: false,
        src: r#"
local function sum(t, n)
  local s = 0.0
  for i = 1, n do s = s + t[i] * 0.5 end
  return s
end
local n = 300
local ti, tf = {}, {}
for i = 1, n do ti[i] = i % 7; tf[i] = (i % 7) + 0.5 end
local total = 0.0
for r = 1, 12 do total = total + sum(ti, n) end
for r = 1, 400 do total = total + sum(tf, n) end
print(string.format("%.6f", total))
"#,
    },
    Prog {
        name: "type_late_stable",
        small: false,
        src: r#"
local function sum(t, n)
  local s = 0.0
  for i = 1, n do s = s + t[i] * 0.5 end
  return s
end
local n = 300
local tf = {}
for i = 1, n do tf[i] = (i % 7) + 0.5 end
local total = 0.0
for r = 1, 412 do total = total + sum(tf, n) end
print(string.format("%.6f", total))
"#,
    },
    Prog {
        name: "tc_livein",
        small: true,
        src: r#"
-- a live-in changes its type at iteration K of a call
local function f(n, k)
  local x = 1
  local s = 0
  for i = 1, n do
    if i == k then x = 1.5 end
    s = s + x
  end
  return s, math.type(x)
end
for _, k in ipairs{1, 2, 3, 100} do
  for rep = 1, 6 do print(k, f(150, k)) end
end
"#,
    },
    Prog {
        name: "tc_elem",
        small: true,
        src: r#"
-- an array element changes its type at index K
local function f(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
for _, k in ipairs{1, 2, 3, 100} do
  local t = {}
  for i = 1, 150 do t[i] = i end
  for rep = 1, 4 do print(k, f(t, 150)) end
  t[k] = k + 0.5
  for rep = 1, 4 do print(k, f(t, 150)) end
end
"#,
    },
    Prog {
        name: "tc_acc",
        small: true,
        src: r#"
-- a loop-carried accumulator turns into a float at iteration K
local function f(n, k)
  local a = 0
  for i = 1, n do
    if i == k then a = a + 0.5 else a = a + 1 end
  end
  return a
end
for _, k in ipairs{1, 2, 3, 100} do
  for rep = 1, 6 do print(k, f(150, k)) end
end
"#,
    },
    Prog {
        name: "tc_callee",
        small: true,
        src: r#"
-- an inlined callee's argument and result change their type at iteration K
local function id(x) return x end
local function f(n, k)
  local s = 0
  for i = 1, n do
    local v = i
    if i >= k then v = i + 0.25 end
    s = s + id(v)
  end
  return s
end
local function half(x) if x > 1e18 then return x * 0.5 end return x // 1 end
local function g(n, k)
  local s = 0
  for i = 1, n do
    s = s + half(i)
    if i == k then s = s + 0.5 end
  end
  return s
end
for _, k in ipairs{1, 2, 3, 100} do
  for rep = 1, 6 do print(k, f(150, k), g(150, k)) end
end
"#,
    },
    Prog {
        name: "metatable_mid",
        small: true,
        src: r#"
local function f(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
for k = 1, 20 do print(f(t, 100)) end
setmetatable(t, {__index = function(_, i) return -1 end})
for k = 1, 5 do print(f(t, 100)) end
t[60] = nil
for k = 1, 5 do print(f(t, 100)) end
setmetatable(t, nil)
print(pcall(f, t, 100))
local u = setmetatable({}, {__index = function(_, i) return i * 2 end})
for k = 1, 20 do print(f(u, 50)) end
"#,
    },
    Prog {
        name: "while_repeat",
        small: true,
        src: r#"
local function w(n)
  local i, s = 0, 0
  while i < n do i = i + 1; if i % 3 == 0 then s = s + i else s = s - 1 end end
  return s
end
local function r(n)
  local i, s = 0, 0.0
  repeat i = i + 1; s = s + i * 0.5 until i >= n
  return s
end
local function b(n)
  local s = 0
  for i = 1, n do if i > 77 then break end; s = s + i end
  return s
end
local function fl(a, bb, c)
  local s = 0.0
  for x = a, bb, c do s = s + x end
  return s
end
for k = 1, 30 do print(w(100 + k), r(90 + k), b(150), fl(1.0, 100.0, 0.5), fl(10, 1, -0.25), fl(0.1, 3, 0.1)) end
"#,
    },
    Prog {
        name: "collectable_stores",
        small: true,
        src: r#"
local function fill(t, n, v)
  for i = 1, n do t[i] = v end
end
local function mv(a, b, n)
  for i = 1, n do a[i] = b[i] end
end
local t, u = {}, {}
for i = 1, 100 do t[i] = i; u[i] = {} end
for k = 1, 20 do
  fill(t, 100, "x")
  fill(t, 100, u)
  mv(t, u, 100)
  fill(t, 100, k)
  collectgarbage()
end
print(#t, t[100], type(u[3]))
local s = {}
for i = 1, 100 do s[i] = tostring(i) end
local function cp(a, b, n) for i = 1, n do a[i] = b[i] end end
local d = {}
for i = 1, 100 do d[i] = 0 end
for k = 1, 20 do cp(d, s, 100); collectgarbage("step") end
print(d[1], d[100], #d)
"#,
    },
    Prog {
        name: "nested_tables",
        small: true,
        src: r#"
local function f(m, n)
  local s = 0
  for i = 1, n do
    local row = m[i]
    for j = 1, #row do s = s + row[j] end
  end
  return s
end
local function g(m, n)
  local s = 0
  for i = 1, n do s = s + m[i][i] end
  return s
end
local m = {}
for i = 1, 30 do m[i] = {} for j = 1, 30 do m[i][j] = i * j end end
for k = 1, 30 do print(f(m, 30), g(m, 30)) end
"#,
    },
    Prog {
        name: "coroutines",
        small: true,
        src: r#"
local function sum(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
local cos = {}
for c = 1, 3 do
  cos[c] = coroutine.wrap(function()
    for k = 1, 40 do coroutine.yield(sum(t, 100) + c) end
    return -1
  end)
end
for k = 1, 41 do print(cos[1](), cos[2](), cos[3]()) end
local co = coroutine.create(function()
  local s = 0
  for i = 1, 200 do s = s + i; if i % 50 == 0 then coroutine.yield(s) end end
  return s
end)
for k = 1, 6 do print(coroutine.resume(co)) end
"#,
    },
    Prog {
        name: "error_late",
        small: true,
        src: r#"
local function f(t, n)
  local s = 0
  for i = 1, n do
    local v = t[i]
    s = s + v
  end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
for k = 1, 20 do f(t, 100) end
t[90] = "x"
print(xpcall(f, debug.traceback, t, 100))
t[90] = {}
print(pcall(f, t, 100))
t[90] = nil
print(pcall(f, t, 100))
local function g(n)
  local s = 0
  for i = 1, n do s = s + i; if i == n then error("late " .. s) end end
end
for k = 1, 10 do print(pcall(g, 100)) end
"#,
    },
    Prog {
        name: "hooks",
        small: true,
        src: r#"
local calls = 0
local function sum(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
for k = 1, 20 do print(sum(t, 100)) end
local function sethook()
  debug.sethook(function() calls = calls + 1 end, "", 50)
end
local function loop(n)
  local s = 0
  for i = 1, n do
    s = s + t[i % 100 + 1]
    if i == 60 then sethook() end
  end
  return s
end
for k = 1, 3 do print(loop(200), calls > 0) end
debug.sethook()
local lines = 0
local function lh() lines = lines + 1 end
local function loop2(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
for k = 1, 20 do loop2(100) end
debug.sethook(lh, "l")
print(loop2(100), lines > 100)
debug.sethook()
for k = 1, 20 do print(loop2(100)) end
"#,
    },
    Prog {
        name: "setlocal",
        small: true,
        src: r#"
local function poke(name, v)
  local i = 1
  while true do
    local n = debug.getlocal(2, i)
    if n == nil then break end
    if n == name then debug.setlocal(2, i, v) end
    i = i + 1
  end
end
local function f(n, k)
  local x = 1
  local s = 0
  for i = 1, n do
    if i == k then poke("x", 2.5) end
    s = s + x
  end
  return s
end
for _, k in ipairs{1, 5, 60, 1000} do
  for rep = 1, 4 do print(k, f(100, k)) end
end
"#,
    },
    Prog {
        name: "deep_recursion",
        small: true,
        src: r#"
local function loop(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function deep(d, n)
  if d == 0 then return loop(n) end
  return deep(d - 1, n) + 1
end
local r = 0
for k = 1, 10 do r = r + deep(10, 100) end
for k = 1, 4 do r = r + deep(300, 100) end
for k = 1, 10 do r = r + deep(10, 100) end
print(r)
for k = 1, 10 do r = r + loop(100) end
print(r)
"#,
    },
    Prog {
        name: "string_compare_abort",
        small: true,
        src: r#"
local function f(t, n)
  local c = 0
  for i = 1, n do
    local v = t[i]
    if v == "a" then c = c + 1 elseif v == 5 then c = c + 2 else c = c + 3 end
  end
  return c
end
local t = {}
for i = 1, 100 do t[i] = (i % 3 == 0) and "a" or i end
for k = 1, 20 do print(f(t, 100)) end
local function g(s, n)
  local c = 0
  for i = 1, n do c = c + #s end
  return c
end
for k = 1, 20 do print(g("hello", 100)) end
"#,
    },
    Prog {
        name: "gc_in_loop",
        small: true,
        src: r#"
local function f(n)
  local keep = {}
  local s = 0
  for i = 1, n do
    local t = {i, i + 1}
    s = s + t[1] + t[2]
    keep[i % 8 + 1] = t
  end
  return s, #keep
end
for k = 1, 20 do print(f(100)) end
local function h(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  if n > 0 then collectgarbage() end
  return s
end
local t = {}
for i = 1, 100 do t[i] = i end
for k = 1, 20 do print(h(t, 100)) end
"#,
    },
    Prog {
        name: "float_forloop",
        small: true,
        src: r#"
local function f(a, b, c)
  local s, n = 0.0, 0
  for x = a, b, c do s = s + x * x; n = n + 1 end
  return s, n
end
for k = 1, 20 do
  print(f(0.0, 100.0, 0.5))
  print(f(1.0, -50.0, -0.75))
  print(f(0, 100, 1))
  print(f(0.1, 0.9, 0.1))
end
local function g(n) local s = 0 for i = n, 1, -3 do s = s + i end return s end
for k = 1, 20 do print(g(100 + k)) end
"#,
    },
    Prog {
        name: "ineff_marking",
        small: false,
        src: r#"
-- the sieve marking loop: the trace recorded for the first prime (step 2) has a step guard in its pre-roll that fails
-- at every entry for the next primes; every back-edge of tier 1 re-enters the trace and leaves at once
local N = 200000
local flags = {}
for i = 1, N do flags[i] = true end
for i = 2, 440 do
  if flags[i] then for j = i * i, N, i do flags[j] = false end end
end
local c = 0
for i = 2, N do if flags[i] then c = c + 1 end end
print(c)
"#,
    },
    Prog {
        name: "sieve_rounds",
        small: false,
        src: r#"
-- the bench sieve (three rounds): the inner loops of the later rounds are re-recorded from identical shapes
local N = 100000
local total = 0
for r = 1, 3 do
  local flags = {}
  for i = 2, N do flags[i] = true end
  for i = 2, math.floor(math.sqrt(N)) do
    if flags[i] then for j = i * i, N, i do flags[j] = false end end
  end
  local c = 0
  for i = 2, N do if flags[i] then c = c + 1 end end
  total = total + c
end
print(total)
"#,
    },
    Prog {
        name: "ineff_marking_stable",
        small: false,
        src: r#"
-- the same loop with a constant step: the trace is healthy (one entry per call, many iterations); 8 rounds, not 40: the
-- `+emurun budget=3` identity job costs ~40k host instructions per Lua instruction (16-25 s at 40 rounds, over the 20 s leash under load)
local N = 200000
local flags = {}
for i = 1, N do flags[i] = true end
for r = 1, 8 do
  for j = 4, N, 2 do flags[j] = false end
end
local c = 0
for i = 2, N do if flags[i] then c = c + 1 end end
print(c)
"#,
    },
    Prog {
        name: "type_flap",
        small: false,
        src: r#"
-- the array type alternates call by call: every trace is invalidated again (re-record, then Black after 3)
local function sum(t, n)
  local s = 0.0
  for i = 1, n do s = s + t[i] * 0.5 end
  return s
end
local n = 300
local ti, tf = {}, {}
for i = 1, n do ti[i] = i % 7; tf[i] = (i % 7) + 0.5 end
local total = 0.0
for r = 1, 400 do total = total + sum(ti, n) + sum(tf, n) end
print(string.format("%.6f", total))
"#,
    },
    Prog {
        name: "inline_exit",
        small: true,
        src: r#"
-- guards INSIDE inlined callees fail mid-loop: the exit rebuilds the callee frames (1 and 2 deep) and the interpreter
-- continues in the callee, returns to the caller and into the loop
local function clamp(x) if x > 100 then return 100 end return x end
local function mid(x) local y = clamp(x) + 1; if y > 50 then y = y - 50 end return y end
local box = 0
local function bump(x) box = box + x; if x > 120 then box = box - 1 end return box end
local function run(n)
  local s = 0
  for i = 1, n do s = s + clamp(i) + mid(i) + bump(i) end
  return s
end
for k = 1, 12 do print(run(60 + k * 10), box) end
local function two(x) return x * 2, x * 3 end
local function run2(n)
  local s = 0
  for i = 1, n do
    local a, b = two(i)
    if i > 70 then a = a + 1 end
    s = s + a + b
  end
  return s
end
for k = 1, 8 do print(run2(50 + k * 10)) end
"#,
    },
    Prog {
        name: "inline_exit_coroutine",
        small: true,
        src: r#"
-- the same traced function entered from fresh coroutines (new CallInfo lists, TR6b) and from the main thread
local function clamp(x) if x > 100 then return 100 end return x end
local function run(n)
  local s = 0
  for i = 1, n do s = s + clamp(i) end
  return s
end
for k = 1, 8 do print(run(90)) end
for k = 1, 6 do
  local co = coroutine.wrap(function(n) return run(n) end)
  print(co(90 + k * 10))
end
local function deep(d, n) if d == 0 then return run(n) end return deep(d - 1, n) end
for k = 1, 4 do print(coroutine.wrap(deep)(5, 120)) end
for k = 1, 4 do print(run(130)) end
"#,
    },
    Prog {
        name: "barrier_stress",
        small: true,
        src: r#"
-- fresh tables are moved from a source array into an old array by a loop; the only reference afterwards is the old
-- array: a missing write barrier lets the collector free them
local function mv(t, src, n)
  for i = 1, n do t[i] = src[i]; src[i] = false end
end
local t, src = {}, {}
for i = 1, 60 do t[i] = false; src[i] = false end
for k = 1, 30 do
  for i = 1, 60 do src[i] = {i, k} end
  mv(t, src, 60)
  local ok = true
  for i = 1, 60 do if t[i][1] ~= i or t[i][2] ~= k then ok = false end end
  collectgarbage("step", 1)
  if k % 10 == 0 then print(k, ok, #t) end
end
local s = {}
for i = 1, 60 do s[i] = false end
local function mvs(dst, n, base)
  for i = 1, n do dst[i] = base .. i end
end
for k = 1, 20 do mvs(s, 60, "k" .. k .. "_") collectgarbage("step", 1) end
print(s[1], s[60])
"#,
    },
    Prog {
        name: "hook_mid_line",
        small: true,
        src: r#"
-- a line hook installed from inside the loop (by a callee) while the loop runs hot: the events counted afterwards
-- are exact, so a trace that does not poll misses them
local lines = 0
local function lh() lines = lines + 1 end
local function arm() debug.sethook(lh, "l") end
local function loop(n, k)
  local s = 0
  for i = 1, n do
    s = s + i
    if i == k then arm() end
  end
  return s
end
for rep = 1, 4 do
  for k = 1, 3 do
    lines = 0
    debug.sethook()
    local r = loop(200, k * 40)
    debug.sethook()
    print(r, lines)
  end
end
"#,
    },
    Prog {
        name: "inline_exit_k",
        small: true,
        src: r#"
-- the exit inside the inlined callee resumes in the interpreter, which reads the callee's constants (kcache)
local function clampf(x) if x > 100.5 then return x * 0.25 + 1.5 end return x end
local function two(x) local y = clampf(x) * 2.5; if y > 400.25 then y = y - 400.125 end return y end
local function runf(n)
  local s = 0.0
  for i = 1, n do s = s + clampf(i + 0.0) + two(i + 0.0) end
  return s
end
for k = 1, 12 do print(string.format("%.9g", runf(60 + k * 12))) end
"#,
    },
    Prog {
        name: "fwd_hazards",
        small: true,
        src: r#"
-- P7: memory forwarding across iterations and the alias rules. Every loop below reads what an earlier iteration (or
-- another table reference) wrote; the same functions run on one table and on two, so a forward through a table ref
-- that "cannot alias" would be visible.
local function chain(a, b, n)              -- a[i+1] = b[i] * 0.5 + 1: forwards only when a and b are one table
  for i = 1, n do a[i + 1] = b[i] * 0.5 + 1.0 end
end
local function behind(t, n)                -- t[i] = t[i-1] + t[i-2]: two values through memory
  for i = 3, n do t[i] = (t[i - 1] + t[i - 2]) % 1000003 end
end
local function ahead(t, n)                 -- reads the next element, writes the current one (anti-dependence)
  for i = 1, n - 1 do t[i] = t[i + 1] + 1 end
end
local function shift(a, b, n)              -- a[i] = b[i + 1]; b[i + 1] = a[i] * 2
  for i = 1, n - 1 do a[i] = b[i + 1]; b[i + 1] = a[i] * 2 end
end
local function twice(a, b, n)              -- read, store through another ref, read again
  local s = 0
  for i = 1, n do
    local x = a[i]
    b[i] = x + 1
    s = s + x + a[i]
  end
  return s
end
local function indirect(t, u, n)           -- the index itself comes from memory
  local s = 0
  for i = 1, n do
    local k = (u[i] & 7) + 1
    t[k] = t[k] + i
    s = s + t[k]
  end
  return s
end
local function delay(t, n)                  -- t[i] = t[i-1] + t[i-3]: a value lives three iterations (PHI chain > 2)
  for i = 4, n do t[i] = (t[i - 1] + t[i - 3]) % 1000003 end
end
local function delayf(t, n)
  for i = 4, n do t[i] = t[i - 1] * 0.5 + t[i - 3] end
end
local function fresh(n) local t = {} for i = 1, n do t[i] = i end return t end
local function freshf(n) local t = {} for i = 1, n do t[i] = i + 0.5 end return t end
for rep = 1, 6 do
  local fa, fb = freshf(60), freshf(60)
  chain(fa, fb, 59); chain(fa, fa, 59); chain(fb, fa, 59)
  local a, b = fresh(60), fresh(60)
  behind(a, 60); behind(b, 60)
  ahead(a, 60); ahead(b, 60)
  shift(a, b, 60); shift(a, a, 60)
  delay(a, 60); delay(b, 60); delayf(fa, 60); delayf(fb, 60)
  local u, v = fresh(60), fresh(60)
  print(rep, twice(a, b, 60), twice(a, a, 60), indirect(u, v, 60), indirect(u, u, 60), a[60], b[60], a[30], b[30], fa[60], fb[30])
  print(a[7], b[11], fa[9], fb[40])
end
"#,
    },
    Prog {
        name: "upval_join_loop",
        small: true,
        src: r#"
-- P7: upvalue accesses through distinct (closure, index) pairs may be one UpVal (debug.upvaluejoin): a store through
-- one is a store to the other
local a, b = 1, 100
local function f1() return a end
local function f2() return b end
local function loop(n)                     -- uses a and b as upvalues 1 and 2
  local s = 0
  for i = 1, n do a = a + 1; s = s + b end
  return s
end
local function loop2(n)
  local s = 0
  for i = 1, n do b = b + a; s = s + a end
  return s
end
for k = 1, 8 do print(loop(50), a, b, loop2(50), a, b) end
debug.upvaluejoin(loop, 2, f1, 1)          -- b of `loop` is now `a`
for k = 1, 8 do print(loop(50), a, b, loop2(50), a, b) end
debug.upvaluejoin(loop2, 1, loop, 1)
for k = 1, 8 do print(loop(50), a, b, loop2(50), a, b) end
"#,
    },
    Prog {
        name: "swap_tables_loop",
        small: true,
        src: r#"
-- P7: the table references are loop carried (swapped every iteration)
local function f(a, b, n)
  local s = 0.0
  for i = 2, n do
    a[i] = b[i - 1] * 0.5 + 1.0
    s = s + a[i - 1]
    a, b = b, a
  end
  return s
end
for rep = 1, 6 do
  local a, b = {}, {}
  for i = 1, 70 do a[i] = i + 0.5; b[i] = i * 0.25 end
  print(f(a, b, 70), a[40], b[40], a[70], b[70])
end
"#,
    },
    Prog {
        name: "ragged_arrays",
        small: true,
        src: r#"
-- P7b G1: two tables of different lengths indexed by the same value. The bounds check of `a[i]` must not stand for the
-- one of `b[i]` (different array sizes): the trace exits at iteration 31 / 61 whichever table is shorter.
local function f(a, b, n)
  local s = 0
  for i = 1, n do s = s + a[i] + b[i] end
  return s
end
local function g(a, b, n)                  -- the missing elements read as 0
  local s = 0
  for i = 1, n do s = s + a[i] + (b[i] or 0) end
  return s
end
local function mk(n) local t = {} for i = 1, n do t[i] = i * 3 end return t end
local function mkf(n) local t = {} for i = 1, n do t[i] = i * 0.5 end return t end
local a60, b30 = mk(60), mk(30)
local f60, f30 = mkf(60), mkf(30)
for rep = 1, 8 do
  print(rep, g(a60, b30, 30), g(a60, b30, 60), g(b30, a60, 30), g(b30, a60, 30), g(f60, f30, 30), g(f30, f60, 30))
  print(pcall(f, a60, b30, 60))
  print(pcall(f, b30, a60, 60))
  print(pcall(f, f60, f30, 60))
  print(pcall(f, f30, f60, 60))
  print(f(a60, a60, 60), f(a60, b30, 30), f(f60, f60, 60))
end
local x, y, z = mk(12), mk(8), mk(5)         -- 12 / 8 / 5
for rep = 1, 6 do
  print(pcall(f, x, y, 12)); print(pcall(f, y, z, 8)); print(pcall(f, z, x, 5)); print(g(x, z, 12), g(y, x, 8), g(z, y, 5))
end
"#,
    },
    Prog {
        name: "for_step_guard",
        small: true,
        src: r#"
-- P7b G3: the trace of a numeric `for` is recorded for one step value (an EQ guard on the step slot): the same loop
-- then runs with other steps, with a zero-trip count and with a zero step.
local function f(a, b, s)
  local acc, k = 0, 0
  for i = a, b, s do acc = acc + i * 3 - k; k = k + 1 end
  return acc, k
end
local function h(t, n, s)                 -- the step also drives an array index
  local acc = 0
  for i = 1, n, s do acc = acc + t[i] end
  return acc
end
local t = {} for i = 1, 100 do t[i] = i * i end
for rep = 1, 6 do
  print(f(1, 100, 1)); print(f(1, 100, 1)); print(f(1, 100, 2)); print(f(100, 1, -1)); print(f(1, 100, 3))
  print(f(5, 4, 1)); print(f(1, 0, 1)); print(f(100, 1, 1)); print(f(1, 100, -1))
  print(pcall(f, 1, 100, 0))
  print(h(t, 100, 1), h(t, 100, 2), h(t, 100, 1), h(t, 99, 7), h(t, 100, 1))
  print(pcall(h, t, 100, 3)) print(pcall(h, t, 101, 1)) print(pcall(h, t, 100, 1))
end
for rep = 1, 3 do                         -- a long stable run, then the step flips back and forth
  local r = 0
  for s = 1, 6 do r = r + f(1, 200, (s & 1) + 1) + h(t, 100, (s % 3) + 1) end
  print(r)
end
"#,
    },
    Prog {
        name: "atouch_hint_sequences",
        small: true,
        src: r#"
-- F7a (TR30): in 5.4 every integer-key array access updates the array limit hint ('alimit_touch'); '#t' observes it
-- (the border search starts from the hint). Traced loops read and write elements at random positions of tables
-- with holes (so '#t' leaves a hinted limit inside the array, BITRAS), '#t' is printed after every loop. In 5.5 the
-- program is just a table workout.
local seed = 12345
local function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648; return seed % n + 1 end
local function rd(t, ks, n)
  local s = 0
  for i = 1, n do s = s + t[ks[i]] end
  return s
end
local function wr(t, ks, n)
  for i = 1, n do local k = ks[i]; t[k] = t[k] + i end
end
local function fixed(t, n)
  local s = 0
  for i = 1, n do s = s + t[9] + t[11] - t[13] end
  return s
end
local out = {}
for round = 1, 70 do
  local n = rnd(40) + 8
  local t, present = {}, {}
  for i = 1, n do t[i] = i; present[i] = true end
  for h = 1, rnd(8) do local k = rnd(n); t[k] = nil; present[k] = false end
  local _ = #t
  local ks, m = {}, 0
  for j = 1, 6 + rnd(18) do
    local k = rnd(n)
    if present[k] then m = m + 1; ks[m] = k end
  end
  if m > 0 then
    out[#out + 1] = rd(t, ks, m)
    out[#out + 1] = #t
    wr(t, ks, m)
    out[#out + 1] = #t
  end
  if present[9] and present[11] and present[13] then
    out[#out + 1] = fixed(t, 8)
    out[#out + 1] = #t
  end
end
print(table.concat(out, ","))
"#,
    },
    Prog {
        name: "for_control_copy",
        small: true,
        src: r#"
-- F7a (TR30): the 5.4 `for` keeps the visible control variable in a copy (R[A+3]) that every iteration refreshes; the
-- body may overwrite it without disturbing the loop (not allowed in 5.5, whose chunks are loaded conditionally).
local src54 = [[
return function(n)
  local s = 0
  for i = 1, n do i = i * 2 + 1; s = s + i end
  return s
end, function(a, b, st)
  local s = 0.0
  for x = a, b, st do x = x - 0.25; s = s + x end
  return s
end]]
local src55 = [[
return function(n)
  local s = 0
  for i = 1, n do local j = i * 2 + 1; s = s + j end
  return s
end, function(a, b, st)
  local s = 0.0
  for x = a, b, st do local y = x - 0.25; s = s + y end
  return s
end]]
local fi, ff = load(_VERSION == "Lua 5.4" and src54 or src55)()
local r = 0
for k = 1, 30 do r = r + fi(100 + k) end
print(r, fi(0), fi(1), fi(1000))
local g = 0.0
for k = 1, 30 do g = g + ff(0.5, 40 + k, 0.5) end
print(string.format("%.6g %.6g %.6g", g, ff(1, 0, 1), ff(10, 1, -0.75)))
local function h(n)
  local acc = 0
  for i = 1, n do acc = acc + i; local j = i; if j > 5 then acc = acc + 1 end end
  return acc
end
local q = 0
for k = 1, 30 do q = q + h(60 + k) end
print(q)
"#,
    },
    // ---- F1: the hash part (HREFK / HLOAD / HSTORE, field forwarding, metatable flag guards) ----
    Prog {
        name: "field_rw",
        small: true,
        src: r#"
-- object fields of every type, read and written in a hot loop (forwarding of a store to the next read)
local o = {i = 0, f = 0.5, s = "str", b = true, t = {}, nf = nil, n = 0}
o.nf = 1
local function step(o, n)
  for k = 1, n do
    o.i = o.i + k
    o.f = o.f * 1.001 + 0.25
    o.n = o.n + 1
    o.b = o.b
    o.s = (k % 2 == 0) and o.s or o.s
    o.t = o.t
  end
  return o.i, o.f, o.n, o.b, o.s
end
local r = 0
for j = 1, 30 do
  local i, f, n, b, s = step(o, 60 + j)
  r = r + i % 1000 + n
end
print(r, step(o, 3))
o.b = not o.b
print(string.format("%.9g", o.f), o.b, o.s, type(o.t), o.nf)
"#,
    },
    Prog {
        name: "field_globals",
        small: true,
        src: r#"
-- globals through the _ENV upvalue: counters, a global function called from the loop (inlined), a global table
G_COUNT = 0
G_SUM = 0.0
G_TAB = {a = 1, b = 2}
function G_ADD(x) G_COUNT = G_COUNT + x end
local function run(n)
  for i = 1, n do
    G_ADD(i)
    G_SUM = G_SUM + i * 0.5
    G_TAB.a = G_TAB.a + 1
    G_TAB.b = G_TAB.b + G_TAB.a
  end
end
for k = 1, 30 do run(50 + k) end
print(G_COUNT, G_SUM, G_TAB.a, G_TAB.b)
"#,
    },
    Prog {
        name: "field_self_methods",
        small: true,
        src: r#"
-- method calls through SELF on a plain table whose methods are fields (no metatable)
local acc = {n = 0, total = 0}
function acc.add(self, x) self.n = self.n + 1; self.total = self.total + x end
function acc.get(self) return self.total end
local function run(a, k)
  for i = 1, k do
    a:add(i)
    if a:get() > 1e9 then a.total = 0 end
  end
  return a:get()
end
local r = 0
for j = 1, 30 do r = r + run(acc, 40 + j) end
print(r, acc.n, acc.total)
"#,
    },
    Prog {
        name: "field_nil_reads",
        small: true,
        src: r#"
-- a field whose value is nil (removed key): reads give nil without a metatable, and with a metatable that has no
-- __index (flag guard); the key comes back later
local plain = {x = 1, y = 2}
local withmt = setmetatable({x = 1, y = 2}, {__tostring = function() return "mt" end, __eq = function() return true end})
local function probe(t, n)
  local c, d = 0, 0
  for i = 1, n do
    local v = t.y
    if v == nil then c = c + 1 else d = d + v end
    if i % 17 == 0 then t.y = nil elseif i % 17 == 9 then t.y = i end
  end
  return c, d
end
plain.y = nil
withmt.y = nil
for j = 1, 20 do
  print(probe(plain, 50 + j))
  print(probe(withmt, 50 + j))
end
print(plain.y, withmt.y, tostring(withmt))
-- the metatables change under the hot loop: the traces (recorded without / with a flag-guarded metatable) must exit
setmetatable(plain, {__index = function(t, k) return 5 end})
print(probe(plain, 60))
getmetatable(withmt).__index = function(t, k) return 6 end
print(probe(withmt, 60))
"#,
    },
    Prog {
        name: "field_key_absent",
        small: true,
        src: r#"
-- reads of keys that are not in the table (no trace may assume the value), insertions through different branches
local t = {a = 1}
local function f(t, n)
  local s = 0
  for i = 1, n do
    s = s + (t.a or 0) + (t.b or 10) + (t.c or 100)
    if i == 25 then t.b = 5 end
    if i == 40 then t.c = 7 end
  end
  return s
end
local r = 0
for j = 1, 10 do
  t = {a = 1}
  r = r + f(t, 60 + j)
end
print(r, t.a, t.b, t.c)
"#,
    },
    Prog {
        name: "field_rehash",
        small: true,
        src: r#"
-- the node vector of the table is rebuilt while the trace is hot (insertions elsewhere in the program), and the same
-- loop is run on tables with the same fields in a different node layout
local function fill(n)
  local t = {p = 1, q = 2, r = 3}
  for i = 1, n do t["k" .. i] = i end
  return t
end
local function hot(t, n)
  local s = 0
  for i = 1, n do
    t.p = t.p + 1
    t.q = t.q + t.p
    s = s + t.r
  end
  return s, t.p, t.q
end
local tabs = {fill(0), fill(1), fill(5), fill(40), {r = 3, q = 2, p = 1}, {q = 2, p = 1, r = 3, z = 0}}
local out = {}
for round = 1, 12 do
  for k = 1, #tabs do
    local s, p, q = hot(tabs[k], 30)
    out[#out + 1] = s + p + q
  end
  tabs[2] = fill(round * 3)
  tabs[1].new = round
end
print(table.concat(out, ","))
"#,
    },
    Prog {
        name: "field_dead_keys",
        small: true,
        src: r#"
-- keys whose values are nil become dead keys when the collector runs: reads, and stores that insert them again
local t = {a = 1, b = 2, c = 3}
local function f(t, n)
  local s = 0
  for i = 1, n do
    s = s + (t.a or 0) + (t.b or 0)
    if i % 11 == 0 then t.b = nil end
    if i % 13 == 0 then t.b = i end
    if i % 7 == 0 then collectgarbage("step", 0) end
  end
  return s
end
local r = 0
for j = 1, 25 do r = r + f(t, 40 + j) end
print(r, t.a, t.b, t.c)
"#,
    },
    Prog {
        name: "field_collectable_stores",
        small: true,
        src: r#"
-- collectable values stored into old tables by a hot loop (the table write barrier of HSTORE)
local holder = {cur = false, str = "", fn = false}
local pool, names = {}, {}
for i = 1, 40 do pool[i] = false; names[i] = false end
local function mv(h, n, k)
  for i = 1, n do
    h.cur = pool[i]
    h.str = names[i]
    pool[i] = false
    names[i] = false
    h.fn = h.cur
  end
end
for k = 1, 25 do
  for i = 1, 40 do pool[i] = {i, k}; names[i] = "n" .. k .. "_" .. i end
  mv(holder, 40, k)
  collectgarbage("step", 1)
  if k % 8 == 0 then print(k, holder.cur[1], holder.cur[2], holder.str, holder.fn == holder.cur) end
end
"#,
    },
    Prog {
        name: "field_metatable_flags",
        small: true,
        src: r#"
-- tables with metatables: existing keys are raw hits whatever the metatable has (__index, __newindex), reads of nil
-- fields are guarded by the metatable's flag cache, and a metatable that gains __index later must stop the trace
local log = 0
local mt1 = {__newindex = function(t, k, v) log = log + 1; rawset(t, k, v) end}
local mt2 = {__index = function(t, k) return -1 end}
local a = {x = 1, y = 2, z = 0}
a.z = nil
setmetatable(a, mt1)
local b = {x = 1, y = 2, z = 0}
b.z = nil
setmetatable(b, {__tostring = function() return "b" end})
local c = setmetatable({x = 1, y = 2}, mt2)
local function f(t, n)
  local s = 0
  for i = 1, n do
    t.x = t.x + 1
    s = s + t.x + (t.y or 0)
  end
  return s
end
local function g(t, n)
  local s = 0
  for i = 1, n do s = s + (t.z or 5) end
  return s
end
local r = 0
for j = 1, 20 do
  r = r + f(a, 30) + f(b, 30) + f(c, 30) + g(a, 30) + g(b, 30)
end
print(r, log, a.x, b.x, c.x)
mt1.__index = function(t, k) return 77 end
print(g(a, 30), g(b, 30))
getmetatable(b).__index = function() return 9 end
print(g(b, 30), g(a, 5))
"#,
    },
    Prog {
        name: "field_string_keys",
        small: true,
        src: r#"
-- constant string keys in registers (GETTABLE / SETTABLE with a LOADK key), fields of upvalue tables, nested fields
local cfg = {width = 10, height = 20, depth = 1}
local kw, kh = "width", "height"
local function area(n)
  local s = 0
  for i = 1, n do
    cfg[kw] = cfg[kw] + 1
    s = s + cfg[kw] * cfg[kh]
    cfg.depth = cfg.depth + (cfg[kh] > 5 and 1 or 0)
  end
  return s
end
local r = 0
for j = 1, 30 do r = r + area(20 + j) end
print(r, cfg.width, cfg.height, cfg.depth)
local deep = {a = {b = {c = 1}}}
local function nest(n)
  for i = 1, n do deep.a.b.c = deep.a.b.c + i end
  return deep.a.b.c
end
local q = 0
for j = 1, 30 do q = q + nest(30) end
print(q)
"#,
    },
    Prog {
        name: "field_shapes",
        small: true,
        src: r#"
-- one loop over objects of the same "class" built with different insertion orders (node layouts) and extra fields
local function mk(order)
  local o = {}
  for _, k in ipairs(order) do o[k] = 0 end
  return o
end
local objs = {
  mk({"x", "y", "z"}), mk({"z", "y", "x"}), mk({"y", "x", "z", "w"}), mk({"x", "y", "z", "a", "b", "c", "d", "e"}),
}
local function bump(o, n)
  for i = 1, n do
    o.x = o.x + 1
    o.y = o.y + o.x
    o.z = o.z - 1
  end
end
for round = 1, 20 do
  for k = 1, #objs do bump(objs[(k + round) % #objs + 1], 25) end
end
for k = 1, #objs do print(objs[k].x, objs[k].y, objs[k].z) end
"#,
    },
    Prog {
        name: "field_layouts",
        small: true,
        src: r#"
-- the same loop over tables that hold the same twelve fields in different node layouts (insertion order, so collision
-- chains differ): the node of a key is a guess of the trace, checked by the key
local names = {}
for i = 1, 12 do names[i] = "k" .. i end
local body = {}
for i = 1, 12 do body[i] = "t." .. names[i] .. " * " .. i end
local rd = load("local t, n = ...; local s = 0; for i = 1, n do s = s + " .. table.concat(body, " + ") .. " end return s")
local function mk(order)
  local t = {}
  for _, i in ipairs(order) do t[names[i]] = i * 10 end
  return t
end
local fwd, rev, mid = {}, {}, {}
for i = 1, 12 do fwd[i] = i; rev[i] = 13 - i; mid[i] = (i * 5) % 13 end
local tabs = {mk(fwd), mk(rev), mk(mid), mk(fwd)}
local out = {}
for round = 1, 8 do
  for k = 1, #tabs do out[#out + 1] = rd(tabs[k], 20 + round) end
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "field_newindex_nil",
        small: true,
        src: r#"
-- a store to a field that exists but is nil goes through __newindex: the trace recorded on a table with a value there
-- must exit (HSTORE old-value check)
local count = 0
local mt = {__newindex = function(t, k, v) count = count + 1; rawset(t, k, v) end}
local function set(t, n) for i = 1, n do t.x = i end end
local t1 = setmetatable({x = 0}, mt)
for r = 1, 30 do set(t1, 20) end
local t2 = setmetatable({x = 0}, mt)
t2.x = nil
for r = 1, 6 do set(t2, 20); t2.x = nil end
print(count, t1.x, t2.x)
local t3 = setmetatable({x = 0}, mt)
for r = 1, 10 do set(t3, 20) end
t3.x = nil
set(t3, 20)
print(count, t3.x)
"#,
    },
    Prog {
        name: "field_alias_tables",
        small: true,
        src: r#"
-- a store through one table reference and a load of the same key through another: a forwarding that ignores that the
-- two references may be one table gives the wrong value
local function f(a, b, n)
  local s = 0
  for i = 1, n do a.v = i; s = s + b.v end
  return s
end
local p, q = {v = 0}, {v = 100}
local r = 0
for k = 1, 20 do r = r + f(p, q, 30) end
print(r, f(p, p, 30), f(q, p, 30), f(p, q, 7), p.v, q.v)
local function g(a, b, n)
  local s = 0
  for i = 1, n do s = s + a.v; b.v = s % 7 end
  return s
end
local u, w = {v = 1}, {v = 2}
for k = 1, 20 do r = r + g(u, w, 25) end
print(r, g(u, u, 25), g(w, u, 25), u.v, w.v)
"#,
    },
    // ---- F2a: side traces (7.8, 7.9) ----
    Prog {
        name: "side_branch",
        small: true,
        src: r#"
-- a branch taken one iteration in three: the recorded path covers one outcome, the other is a hot exit and gets a side
-- trace that ends at the loop head (a link into the root trace)
local function f(n)
  local s = 0
  for i = 1, n do
    if i % 3 == 0 then s = s + i * 2 else s = s - 1 end
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(60 + k) end
print(r, f(1), f(2), f(3), f(100))
"#,
    },
    Prog {
        name: "side_nested",
        small: true,
        src: r#"
-- two levels of branches: a side trace has side exits of its own (a side trace of a side trace)
local function f(n)
  local s, c = 0, 0
  for i = 1, n do
    if i % 2 == 0 then
      if i % 4 == 0 then s = s + i else s = s - 3 end
    else
      c = c + 1
      if i % 5 == 0 then s = s * 2 % 1000003 end
    end
  end
  return s, c
end
local a, b = 0, 0
for k = 1, 30 do local x, y = f(80 + k) a = a + x b = b + y end
print(a, b, f(7))
"#,
    },
    Prog {
        name: "side_entry0",
        small: true,
        src: r#"
-- the type_entry shape: the first iteration of every inner run sees an integer accumulator, so the root trace's entry
-- check fails once per entry (a hot "exit 0"): an entry-0 side trace runs that iteration and links into the root
local function run(t, n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local m = 40
local t = {}
for j = 1, m do t[j] = (j % 9) * 0.25 end
local total = 0.0
for r = 1, 12 do total = total + run(t, 20, m) end
print(string.format("%.6f", total), run(t, 1, 1), run(t, 3, 2))
"#,
    },
    Prog {
        name: "side_entry0_mixed",
        small: true,
        src: r#"
-- the inner loop is entered with an integer accumulator in two runs out of three and with a float in the third: the
-- entry-0 side trace (the head's prelude) serves the integer runs and hands the float ones to the root trace, whose
-- entry it falls into when its own entry checks fail
local function run(t, n, m)
  local out = 0.0
  for i = 1, n do
    local a = 0
    if i % 3 == 0 then a = 0.5 end
    for j = 1, m do a = a + t[j] * 0.5 end
    out = out + a
  end
  return out
end
local m = 30
local t = {}
for j = 1, m do t[j] = (j % 7) * 0.25 end
local total = 0.0
for r = 1, 16 do total = total + run(t, 24, m) end
print(string.format("%.6f", total), run(t, 2, 1), run(t, 4, 3))
"#,
    },
    Prog {
        name: "side_midblock",
        small: true,
        src: r#"
-- exits in the middle of a basic block (the table read and the compare in the middle of a long straight-line body):
-- the side trace starts at a mid-block tier-1 label whose rest of the block was prepaid (count model of 3.7)
local function f(t, n)
  local s, u = 0, 1
  for i = 1, n do
    local a = t[i] + 1
    u = u * 3 % 1009
    s = s + a * u - i
    if a > 5 then s = s - 7 end
    u = u + 2
    s = s + u % 11
  end
  return s
end
local t = {}
for i = 1, 90 do t[i] = (i * 7) % 11 end
local r = 0
for k = 1, 30 do r = r + f(t, 30 + k) end
print(r, f(t, 1), f(t, 2), f(t, 90))
"#,
    },
    Prog {
        name: "side_calls",
        small: true,
        src: r#"
-- the callee at a call site changes: the identity guard exits and a side trace inlines the other callee
local function inc(x) return x + 1 end
local function dbl(x) return x * 2 % 997 end
local fs = { inc, dbl, inc }
local function f(n)
  local s = 0
  for i = 1, n do
    local g = fs[i % 3 + 1]
    s = s + g(i)
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(60 + k) end
print(r, f(1), f(2), f(5))
"#,
    },
    Prog {
        name: "side_to_other_head",
        small: true,
        src: r#"
-- the break exit of the first loop leads into the second loop's head, which has a trace: the side trace ends there (a
-- link into a trace of another head through its dispatch cell)
local function f(lim, n)
  local s, i = 0, 0
  while true do
    i = i + 1
    if i > lim then break end
    s = s + i
  end
  local j = 1
  while j <= n do
    s = s + j * 2
    j = j + 1
  end
  return s
end
local r = 0
for k = 1, 40 do r = r + f(60 + k % 7, 50 + k % 5) end
print(r, f(0, 0), f(1, 1), f(100, 3))
"#,
    },
    Prog {
        name: "side_untraced_head",
        small: true,
        src: r#"
-- the recording of a side trace reaches a loop head that has no trace: it aborts (7.8) and the exit keeps landing in
-- tier 1
local function f(n)
  local s = 0
  for i = 1, n do
    if i % 4 == 0 then
      local k = 0
      while k < 3 do s = s + #string.rep('z', k + 1); k = k + 1 end
    else
      s = s + 1
    end
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(50 + k) end
print(r, f(4), f(8))
"#,
    },
    Prog {
        name: "side_tr6d_entry0",
        small: true,
        src: r#"
-- TR6d for entry-0 side traces: the closure called through `acc`'s neighbour is read from an upvalue after a store to an
-- upvalue that debug.upvaluejoin has made an alias of it; the identity guard must keep its own (mid-iteration) snapshot
local acc = 0
local function a1(x) return x + 1 end
local function a2(x) return x + 2 end
local fn = a1
local function setfn(g) fn = g end
local function loop(n, first)
  local a = first
  for i = 1, n do
    acc = acc + 1
    a = a + fn(i)
  end
  return a
end
for k = 1, 12 do print(loop(30, 0), acc) end
setfn(a2)
for k = 1, 12 do print(loop(30, 0.5), acc) end
local function g() return fn end
debug.upvaluejoin(loop, 1, g, 1)   -- `acc` of loop is now `fn` of g: acc = acc + 1 stores into the callee slot
for k = 1, 4 do print(pcall(loop, 20, 0)) end
"#,
    },
    Prog {
        name: "side_poly_types",
        small: false,
        src: r#"
-- an array of alternating integers and floats: the element type guard of the root trace fails every other iteration
local function f(t, n)
  local s = 0
  for i = 1, n do s = s + t[i] end
  return s
end
local t = {}
for i = 1, 400 do if i % 2 == 0 then t[i] = i else t[i] = i + 0.5 end end
local r = 0
for k = 1, 60 do r = r + f(t, 400) end
print(r)
"#,
    },
    Prog {
        name: "side_many",
        small: false,
        src: r#"
-- many hot exits in one loop (a family with a lot of side traces)
local function f(n)
  local s = 0
  for i = 1, n do
    local m = i % 11
    if m == 0 then s = s + 1
    elseif m == 1 then s = s + 2
    elseif m == 2 then s = s + 3
    elseif m == 3 then s = s - 4
    elseif m == 4 then s = s + 5
    elseif m == 5 then s = s - 6
    elseif m == 6 then s = s + 7
    elseif m == 7 then s = s - 8
    elseif m == 8 then s = s + 9
    else s = s + 10 end
  end
  return s
end
local r = 0
for k = 1, 40 do r = r + f(500 + k) end
print(r)
"#,
    },
    // ---- F2b: linking --------------------------------------------------------------------------------------------
    Prog {
        name: "nest_mandel",
        small: true,
        src: r#"
-- a short inner while loop in an outer for loop (mandelbrot): the outer loop's trace links into the inner one, whose
-- loop exits get side traces that link back to the outer trace
local function mandel(n)
  local count = 0
  for y = 0, 3 do
    local ci = 2.0 * y / 4 - 1.0
    for x = 0, n - 1 do
      local cr = 2.0 * x / n - 1.5
      local zr, zi, i = 0.0, 0.0, 0
      while i < 30 and zr * zr + zi * zi < 4.0 do zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci; i = i + 1 end
      if i == 30 then count = count + 1 end
    end
  end
  return count
end
print(mandel(300), mandel(200))
"#,
    },
    Prog {
        name: "nest_for_for",
        small: true,
        src: r#"
-- numeric `for` loops nested (FORPREP on the outer trace's path): steps 1, -1 and 3, skipped loops, a loop that ends
-- inside side traces (FORLOOP with count 0)
local function f(n, st)
  local s = 0
  for i = 1, n do
    for j = 1, 20, st do s = s + j end
    for j = 1, 5 do s = s + i * j end
    for j = 9, 1, -1 do s = s - j end
    for j = 1, 12, 3 do s = s + j end
    for j = 3, 1 do s = s + 1000 end
    for j = i, i + 2 do s = s + j end
  end
  return s
end
local r = 0
for k = 1, 25 do r = r + f(60 + k, 1) end
for k = 1, 25 do r = r + f(60 + k, 1 + k % 3) end
print(r, f(0, 1), f(1, 2))
"#,
    },
    Prog {
        name: "pf_var_step",
        small: true,
        src: r#"
-- P-F: loops whose step is computed at run time (not a literal of the prototype): steps of both signs, the sign changing
-- between runs of the same loop, skipped loops, huge steps, step 0 and mininteger (errors / Nyi)
local function f(n, st, a, b)
  local s = 0
  for i = 1, n do
    for j = a, b, st do s = s + j end
    for j = i * 2, b + i, st do s = s - j end
  end
  return s
end
local r = 0
for k = 1, 40 do r = r + f(30 + k, 1 + k % 4, 1, 60) end
for k = 1, 40 do r = r + f(30 + k, -(1 + k % 5), 60, 1) end
for k = 1, 60 do
  local st = (k % 2 == 0) and (2 + k % 3) or -(2 + k % 3)
  if st > 0 then r = r + f(20, st, 1, 40) else r = r + f(20, st, 40, 1) end
end
print(r, f(5, 1 << 40, 1, 1 << 42), f(5, -(1 << 40), -1, -(1 << 42)), f(0, 1, 1, 2), f(3, 7, 9, 1))
-- a difference that does not fit a signed integer (the unsigned count is 2): not recorded
for k = 1, 30 do r = r + f(5, math.maxinteger // 2, -5, math.maxinteger) + f(5, math.mininteger // 2, 5, math.mininteger) end
print(r)
for k = 1, 40 do r = r + f(20, 3, 1, 40) end
for k = 1, 5 do r = r + f(20, -3, 1, 40) + f(20, 3, 40, 1) end -- the other sign: no iterations at all
for k = 1, 40 do r = r + f(20, -3, 40, 1) end
for k = 1, 5 do r = r + f(20, 3, 1, 40) + f(20, -3, 1, 40) end
print(r)
print(pcall(f, 3, 0, 1, 5))
print(pcall(f, 3, math.mininteger, 5, 1))
print(pcall(f, 3, math.maxinteger, 1, math.maxinteger))
local flags = {}
for i = 1, 3000 do flags[i] = true end
for i = 2, 54 do if flags[i] then for j = i * i, 3000, i do flags[j] = false end end end
local c = 0
for i = 2, 3000 do if flags[i] then c = c + 1 end end
print(c)
"#,
    },
    Prog {
        name: "nest_float_limit",
        small: true,
        src: r#"
-- a float limit and a float loop inside an integer nest: FORPREP is not recorded for them (Nyi), the program must still
-- run identically
local function f(n)
  local s = 0
  for i = 1, n do
    for j = 1, n / 40 do s = s + j end
    for x = 0.5, 3.5, 1.0 do s = s + x end
  end
  return s
end
local r = 0
for k = 1, 25 do r = r + f(80) end
print(r)
"#,
    },
    Prog {
        name: "nest_frame",
        small: true,
        src: r#"
-- the inner loop is in a callee: with linking exits (nest=2) the caller's trace materialises the callee's frame and
-- continues in the callee's loop trace (heapsort's `sift`)
local function sift(a, lo, hi)
  local root = lo
  while true do
    local child = root * 2
    if child > hi then break end
    if child < hi and a[child] < a[child + 1] then child = child + 1 end
    if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
  end
end
local function heapsort(a, n)
  for i = n // 2, 1, -1 do sift(a, i, n) end
  for i = n, 2, -1 do a[1], a[i] = a[i], a[1]; sift(a, 1, i - 1) end
end
local seed, ok = 42, true
for r = 1, 12 do
  local a, n = {}, 150 + r
  for i = 1, n do seed = (seed * 1103515245 + 12345) % 2147483648; a[i] = seed end
  heapsort(a, n)
  for i = 2, n do if a[i - 1] > a[i] then ok = false end end
end
print(ok)
"#,
    },
    Prog {
        name: "nest_frame_scan",
        small: true,
        src: r#"
-- an array scan in a callee, called from a loop (linking exit into the callee's loop trace)
local function scan(a, lo, hi)
  local i, s = lo, 0
  while i <= hi do
    local v = a[i]
    if v % 3 == 0 then a[i] = v + 1 end
    s = s + v
    i = i + 1
  end
  return s
end
local function run(a, n)
  local t = 0
  for i = 1, n do t = t + scan(a, i, n) end
  return t
end
local a = {}
for i = 1, 60 do a[i] = i * 7 % 23 end
local r = 0
for k = 1, 8 do r = r + run(a, 60) end
print(r)
"#,
    },
    Prog {
        name: "nest_frame_pixel",
        small: true,
        src: r#"
-- a callee with an escape loop called from a loop; the callee returns after its loop (the side traces of its exits
-- reach the end of the frame)
local function pixel(cr, ci)
  local zr, zi, k = 0.0, 0.0, 0
  while k < 12 and zr * zr + zi * zi < 4.0 do
    zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci
    k = k + 1
  end
  return k
end
local function row(ci, w)
  local total = 0
  for x = 0, w - 1 do total = total + pixel(-2.0 + x * (3.0 / w), ci) end
  return total
end
local t = 0
for r = 1, 60 do t = t + row(((r % 16) - 8) / 8.0, 40) end
print(t)
"#,
    },
    Prog {
        name: "nest_stitch",
        small: true,
        src: r#"
-- a long loop body with a C function call in it (stitching: the trace covers the body up to the call)
local function f(n)
  local a, b, c, d, s = 1, 2, 3, 4, 0
  for i = 1, n do
    a = a + i; b = b + a; c = c ~ b; d = d + c % 7
    a = a * 3 % 1000; b = b + d; c = c + a; d = d - 1
    local k = #string.rep("y", i % 5 + 1) -- (P-B: `tostring` of a number is an intrinsic now: not a C call)
    s = s + k + a + b % 3
    if i % 50 == 0 then s = s + #string.rep("x", i % 7) end
  end
  return s
end
local r = 0
for j = 1, 40 do r = r + f(300) end
print(r)
"#,
    },
    Prog {
        name: "fcmp_nan_const",
        small: true,
        src: r#"
-- float compares with a constant operand where a NaN arrives in a later iteration than the one that was recorded: the
-- trace's guard must leave on NaN (a negated condition would not), and the compared register must survive the guard
local function f(n)
  local s, z = 0, 0.0
  for i = 1, n do
    local v = i * 0.5
    if i % 7 == 0 then v = z / z end
    if v < 3.0 then s = s + 1 end
    if 2.0 <= v then s = s + 2 end
    if v > 1.5 then s = s + 4 end
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(40 + k) end
print(r)
"#,
    },
    Prog {
        name: "nest_dep",
        small: true,
        src: r#"
-- the prototype the inner loop's trace inlines is freed (the trace is flushed through its dependency edge) while the
-- outer trace that links into the inner loop stays installed: its link goes through the head's cell, which tier 1's
-- counting stub owns again; a new inner trace is recorded and linked later
local function newfn(c) return load("local c = ... return function(v) return v + c end")(c) end
local h = newfn(1)
local function run(n)
  local s = 0
  for i = 1, n do
    local k = 0
    while k < 4 do s = s + h(k); k = k + 1 end
  end
  return s
end
local r = 0
for round = 1, 6 do
  for k = 1, 12 do r = r + run(100) end
  h = newfn(round + 1)
  collectgarbage()
  collectgarbage()
end
print(r)
"#,
    },
    Prog {
        name: "nest_tr6d",
        small: true,
        src: r#"
-- the inner loop (entered from the outer trace through its dispatch cell) calls the closure in upvalue `fn` after an
-- upvalue store; between phases `fn` changes, so the link's target must re-check its callee identity at every entry
local cnt = 0
local function p1(x) return x + 1 end
local function p2(x) return x + 2 end
local fn = p1
local function setfn(g) fn = g end
local function loop(n, first)
  local a = first
  for i = 1, n do
    local k = 0
    while k < 3 do
      cnt = cnt + 1
      a = a + fn(k)
      k = k + 1
    end
  end
  return a
end
for r = 1, 12 do print(loop(30, 0), cnt) end
setfn(p2)
for r = 1, 12 do print(loop(30, 0.5), cnt) end
setfn(p1)
for r = 1, 6 do print(loop(30, 0), cnt) end
"#,
    },
    Prog {
        name: "nest_types",
        small: true,
        src: r#"
-- the outer trace's entry checks fail after a type change (the link target re-checks its own entry); the inner loop is
-- traced first, the outer one links
local function f(n, step)
  local s = 0
  for i = 1, n do
    local k = 0
    local a = i * step
    while k < 5 do s = s + a; k = k + 1 end
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(200, 2) end
r = r + f(300, 1.5) + f(300, 3)
print(r)
"#,
    },
    Prog {
        name: "nest_flush",
        small: true,
        src: r#"
-- the inner loop's callee is replaced (its trace is flushed through the dependency edge) while the outer trace, which
-- links into it, stays installed
local function mk(c)
  return function(v) return v + c end
end
local f = mk(1)
local function run(n)
  local s = 0
  for i = 1, n do
    local k = 0
    while k < 4 do s = s + f(k); k = k + 1 end
  end
  return s
end
local r = 0
for k = 1, 20 do r = r + run(150) end
f = mk(2)
for k = 1, 20 do r = r + run(150) end
f = mk(3)
r = r + run(500)
print(r)
"#,
    },
    // ---- F3: class-A helpers (TR1): NEWTABLE, SETLIST, NEWREF, LEN ----
    Prog {
        name: "alloc_ctor_array",
        small: true,
        src: r#"
-- array constructors in a loop: TNEW + SETLIST (fixed count), element reads, length
local function f(n)
  local s = 0
  for i = 1, n do
    local t = {i, i + 1, i * 2}
    s = s + t[1] + t[2] + t[3] + #t
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(50 + k) end
print(r, f(0), f(1), f(1000))
"#,
    },
    Prog {
        name: "alloc_ctor_hash",
        small: true,
        src: r#"
-- hash constructors and field insertion into preallocated and empty hash parts (rehash on the way), with a collectable
-- value stored into an older table (barrier)
local function f(n)
  local s = 0
  local keep = {false, false, false, false}
  for i = 1, n do
    local o = {x = i, y = i * 2}
    o.z = i + 1
    o.w = o
    local e = {}
    e.a = i
    e.b = o
    e.c = "s"
    keep[1 + i % 4] = o
    s = s + o.x + o.y + o.z + e.a + e.b.x + o.w.x
  end
  return s, #keep
end
local a, b = 0, 0
for k = 1, 30 do local x, y = f(40 + k) a = a + x b = b + y end
print(a, b, f(3))
"#,
    },
    Prog {
        name: "alloc_len",
        small: true,
        src: r#"
-- the length of arrays (with a trailing nil), strings, and tables with __len
local function f(t, s, n)
  local r = 0
  for i = 1, n do r = r + #t + #s end
  return r
end
local t1 = {1, 2, 3, 4, 5}
local t2 = {1, 2, 3, nil, 5}
local t3 = setmetatable({1, 2, 3}, {__len = function() return 10 end})
local t4 = setmetatable({1, 2, 3}, {})
local e = {}
local out = {}
for round = 1, 6 do
  out[#out + 1] = f(t1, "hello", 30)
  out[#out + 1] = f(t4, string.rep("x", 100), 30)
  out[#out + 1] = f(e, "", 30)
end
out[#out + 1] = f(t2, "ab", 20)
out[#out + 1] = f(t3, "ab", 20)
out[#out + 1] = f(t1, "hello", 30)
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "alloc_newref_present",
        small: true,
        src: r#"
-- the key is absent in the recorded run and present (or present with a nil value) in later runs: NEWREF does nothing
-- and the trace exits; the interpreter stores
local function f(o, n)
  local s = 0
  for i = 1, n do
    o.k = i
    s = s + o.k
  end
  return s
end
local r = 0
for round = 1, 30 do
  local t = {}
  r = r + f(t, 20)
  local u = {k = 5}
  r = r + f(u, 20)
  local v = {k = 1}
  v.k = nil
  r = r + f(v, 20)
end
print(r)
"#,
    },
    Prog {
        name: "alloc_newindex",
        small: true,
        src: r#"
-- a table with __newindex (function and table) and with a metatable lacking it: insertion must not bypass the metamethod
local seen = 0
local mt1 = {__newindex = function(t, k, v) seen = seen + 1; rawset(t, k, v) end}
local mt2 = {}
local side = {}
local mt3 = {__newindex = side}
local function f(o, n)
  local s = 0
  for i = 1, n do
    o.q = i
    s = s + 1
  end
  return s
end
for round = 1, 30 do
  f(setmetatable({}, mt2), 10)
end
f(setmetatable({}, mt1), 10)
f(setmetatable({}, mt3), 10)
local t = setmetatable({}, mt1)
f(t, 10)
print(seen, side.q, rawget(t, "q"))
"#,
    },
    Prog {
        name: "alloc_escape",
        small: true,
        src: r#"
-- tables allocated in the loop are stored into older tables (and into upvalue-held tables): the barrier, the aging
local keep = {}
local function f(n)
  local last
  for i = 1, n do
    local t = {i}
    local u = {t = t, n = i}
    keep[i % 16 + 1] = u
    last = u
  end
  return last.n, last.t[1]
end
local r = 0
for k = 1, 40 do local a, b = f(30 + k) r = r + a + b end
local s = 0
for i = 1, #keep do s = s + keep[i].n + keep[i].t[1] end
print(r, s)
"#,
    },
    Prog {
        name: "alloc_churn",
        small: false,
        src: r#"
-- many short-lived tables (the collector steps often): results and the number of live bytes stay sane
local function f(n)
  local s = 0
  for i = 1, n do
    local t = {i, i + 1, x = i, y = i}
    local u = {}
    u.a = t
    u.b = i
    s = s + u.a[1] + u.b + u.a.x
  end
  return s
end
local r = 0
for k = 1, 60 do r = r + f(500) end
collectgarbage()
print(r, collectgarbage("count") < 100000)
"#,
    },
    Prog {
        name: "alloc_churn_gen",
        small: false,
        src: r#"
collectgarbage("generational")
-- many short-lived tables (the collector steps often): results and the number of live bytes stay sane
local function f(n)
  local s = 0
  for i = 1, n do
    local t = {i, i + 1, x = i, y = i}
    local u = {}
    u.a = t
    u.b = i
    s = s + u.a[1] + u.b + u.a.x
  end
  return s
end
local r = 0
for k = 1, 60 do r = r + f(500) end
collectgarbage()
print(r, collectgarbage("count") < 100000)
"#,
    },
    Prog {
        name: "alloc_churn_inc",
        small: false,
        src: r#"
collectgarbage("incremental")
-- many short-lived tables (the collector steps often): results and the number of live bytes stay sane
local function f(n)
  local s = 0
  for i = 1, n do
    local t = {i, i + 1, x = i, y = i}
    local u = {}
    u.a = t
    u.b = i
    s = s + u.a[1] + u.b + u.a.x
  end
  return s
end
local r = 0
for k = 1, 60 do r = r + f(500) end
collectgarbage()
print(r, collectgarbage("count") < 100000)
"#,
    },
    Prog {
        name: "alloc_large_ctor",
        small: true,
        src: r#"
-- constructors larger than the preallocated array of NEWTABLE (SETLIST batches), nils in the list, nested tables
local function f(n)
  local s = 0
  for i = 1, n do
    local t = {i, i, i, i, i, i, i, i, i, i, i, i, nil, i}
    local u = {{i}, {i + 1}, {x = i}}
    s = s + #u + (u[1][1] + u[2][1] + u[3].x) + t[14] + t[12]
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(40 + k) end
print(r, f(2))
"#,
    },
    Prog {
        name: "alloc_proto_churn",
        small: true,
        src: r#"
-- TR32: the recorded iteration inlines a callee whose only references are then dropped, and the NEWTABLE that follows
-- runs a GC step (stress mode) that frees the callee's closure and prototype while the recording is still open. The
-- recording must abort (DepFreed) instead of keeping a callee-identity guard on a freed prototype. Later rounds load
-- new functions (their prototypes may reuse the freed addresses) with different bodies.
local N = 8
local function mk(k) return load("return function(x) return x * " .. k .. " + 1 end")() end
local function run(fs, n)
  local acc = 0
  for i = 1, n do
    local f = fs[i]
    acc = f(acc) % 1000003
    fs[i] = false
    f = nil
    local t = {}
  end
  return acc
end
local out = {}
for round = 1, 7 do
  local fs = {}
  for i = 1, N do fs[i] = mk(round * 10 + i) end
  out[#out + 1] = run(fs, N)
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "alloc_root_before_callee",
        small: true,
        src: r#"
-- TR32: a trace whose root prototype is freed before the callee it inlined: the root's flush runs first, the callee's
-- later free finds a stale edge (no second flush, no flush of a reused slot)
local function mkg(k) return load("return function(x) return x + " .. k .. " end")() end
local out = {}
for round = 1, 6 do
  local g = mkg(round)
  local root = load("local g = ... return function(n) local s = 0 for i = 1, n do s = s + g(i) end return s end")(g)
  for k = 1, 12 do out[#out + 1] = root(25) end
  root = nil
  collectgarbage()
  g = nil
  collectgarbage()
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "alloc_interleaved",
        small: true,
        src: r#"
-- inserts and reads of one table interleaved: every insertion may rehash (the node vector moves), so a field read after
-- an insertion must not reuse the node pointer / size of a read before it
local function f(n)
  local s = 0
  for i = 1, n do
    local e = {}
    e.a = i
    s = s + e.a
    e.b = i * 2
    s = s + e.a + e.b
    e.c = i * 3
    s = s + e.a + e.b + e.c
    e.d = i * 4
    e.e = i * 5
    s = s + e.a + e.e + e.d + e.c
  end
  return s
end
local r = 0
for k = 1, 30 do r = r + f(30 + k) end
print(r, f(1))
"#,
    },
    Prog {
        name: "alloc_append",
        small: true,
        src: r#"
-- table building: t[i] = v past the array part (growth through rehash), t[#t + 1] = v, sparse integer keys (hash part), values
-- of every type, and a table filled twice (the second time the keys are present)
local function build(n)
  local t, u, w, x = {}, {}, {}, {}
  for i = 1, n do t[i] = i * 2 end
  for i = 1, n do u[#u + 1] = i + 0.5 end
  for i = 1, n do w[i * 7] = i end
  for i = 1, n do x[i] = (i % 3 == 0) and "s" or ((i % 3 == 1) and {i} or true) end
  for i = 1, n do t[i] = t[i] + 1 end
  local s = 0
  for i = 1, n do s = s + t[i] + u[i] end
  for i = 1, n do local v = w[i * 7]; if v then s = s + v end end
  return s, #t, #u, #x, type(x[1]), type(x[2]), type(x[3])
end
local r = 0
for k = 1, 30 do local a, b, c, d = build(20 + k) r = r + a + b + c + d end
print(r, build(1))
print(build(100))
"#,
    },
    Prog {
        name: "alloc_append_meta",
        small: true,
        src: r#"
-- integer-key stores into tables with __newindex (function, table) and with a metatable that lacks it
local seen = 0
local side = {}
local function fill(t, n)
  for i = 1, n do t[i] = i end
  return #t
end
local out = {}
for round = 1, 20 do
  out[#out + 1] = fill(setmetatable({}, {}), 12)
  out[#out + 1] = fill({}, 12)
end
local mt = {__newindex = function(t, k, v) seen = seen + 1; rawset(t, k, v) end}
out[#out + 1] = fill(setmetatable({}, mt), 12)
out[#out + 1] = fill(setmetatable({}, {__newindex = side}), 12)
out[#out + 1] = seen
out[#out + 1] = #side
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "alloc_pool",
        small: true,
        src: r#"
-- insertions into tables that already exist (no allocation in the loop), each insertion may rehash: a field written after
-- an insertion must reach the table's current node vector, not the one a read before the insertion found
local function f(pool, n)
  local s = 0
  for i = 1, n do
    local o = pool[i]
    o.a = i
    o.b = i * 2
    o.a = o.a + o.b
    o.c = o.a
    s = s + o.a + o.c
  end
  return s
end
local function mk(n) local p = {} for i = 1, n do p[i] = {} end return p end
local r = 0
for round = 1, 30 do
  local pool = mk(40)
  r = r + f(pool, 40)
  local t = 0
  for i = 1, 40 do t = t + pool[i].a + pool[i].b + pool[i].c end
  r = r + t
end
print(r)
"#,
    },
    Prog {
        name: "tfor_ipairs",
        small: true,
        src: r#"
-- ipairs over arrays: one and two loop variables, sums, a stored element, an early exit, a table that gets a hole, and
-- a metatable-free table that grows while it is iterated
local function sum2(t) local s = 0 for i, v in ipairs(t) do s = s + i * v end return s end
local function sum1(t) local n = 0 for i in ipairs(t) do n = n + i end return n end
local function strs(t) local s = 0 for _, v in ipairs(t) do s = s + #v end return s end
local function dbl(t) for i, v in ipairs(t) do t[i] = v * 2 end return t[#t] end
local function first(t, x) for i, v in ipairs(t) do if v == x then return i end end return -1 end
local function three(t) local s = 0 for i, v, w in ipairs(t) do s = s + i + (w or 7) end return s end
local a, b, c = {}, {}, {}
for i = 1, 200 do a[i] = i % 17; b[i] = (i % 5 == 0) and "xy" or "z"; c[i] = i + 0.5 end
local r = 0
for k = 1, 30 do
  r = r + sum2(a) + sum1(a) + strs(b) + dbl(a) + first(a, 9) + three(a)
  r = r + sum2(c) + first(c, 100.5)
end
print(r, sum2({}), sum1({1}), first({}, 1), #a)
local h = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10}
local m = 0
for round = 1, 40 do
  h[(round % 10) + 1] = nil
  m = m + sum2(h) + sum1(h)
  h[(round % 10) + 1] = round
end
print(m)
local mt = setmetatable({}, {__index = function(_, i) if i <= 5 then return i * 3 end end})
local u = 0
for round = 1, 30 do u = u + sum2(mt) + sum2(a) end
print(u)
"#,
    },
    Prog {
        name: "tfor_closure",
        small: true,
        src: r#"
-- generic `for` over Lua closure iterators (BUNDLE-Q): upvalue state, early break, a nil / false / extra results, more loop
-- variables than results, a stateless iterator with explicit state and control, errors inside the iterator, a closing value,
-- nested iterators, a coroutine.wrap iterator, an iterator that changes behaviour (type) mid-run
local function range(n) local i = 0 return function() i = i + 1 if i <= n then return i end end end
local function range2(n) local i = 0 return function() i = i + 1 if i <= n then return i, i * i, "x" end end end
local function stateless(t, i) i = i + 1 local v = t[i] if v ~= nil then return i, v end end
local function falsy(n) local i = 0 return function() i = i + 1 if i <= n then return i % 2 == 0 end end end
local function mixed(n) local i = 0 return function() i = i + 1 if i <= n then if i % 3 == 0 then return i + 0.5 end return i end end end
local function strs(n) local i = 0 return function() i = i + 1 if i <= n then return (i % 2 == 0) and "ab" or "c" end end end
local function sum(n) local s = 0 for x in range(n) do s = s + x end return s end
local function sum2(n) local s = 0 for a, b, c in range2(n) do s = s + a + b + #c end return s end
local function brk(n, k) local s = 0 for x in range(n) do if x > k then break end s = s + x end return s end
local function ret(n, k) for x in range(n) do if x == k then return x * 2 end end return -1 end
local function four(n) local s = 0 for a, b, c, d in range2(n) do s = s + a + (d == nil and 1 or 0) end return s end
local function sless(t) local s = 0 for i, v in stateless, t, 0 do s = s + i * v end return s end
local function fl(n) local c = 0 for v in falsy(n) do if v then c = c + 1 end end return c end
local function mx(n) local s = 0 for v in mixed(n) do s = s + v end return s end
local function st(n) local s = 0 for v in strs(n) do s = s + #v end return s end
local function nest(n) local s = 0 for i in range(n) do for j in range(i) do s = s + j end end return s end
local r = 0
for k = 1, 60 do
  r = r + sum(50 + k % 7) + sum2(20) + brk(40, k % 30) + ret(30, k % 40) + four(10) + fl(30) + sless({1, 2, 3, 4, 5, 6, 7, 8}) + st(25)
  r = r + mx(40) + nest(12)
end
print(r, sum(0), sum(1), sum2(0), brk(5, 0), ret(3, 9), fl(0), mx(1), st(0), nest(0))
-- the iterator ends at different lengths per call (side exits on the end of the loop)
local t = 0
for k = 1, 200 do t = t + sum(k % 13) end
print(t)
-- errors raised inside the iterator, at a few positions
local function bad(n) local i = 0 return function() i = i + 1 if i == n then error("boom " .. i) end return i end end
local e = 0
for k = 1, 40 do
  local ok, msg = pcall(function() local s = 0 for x in bad(5 + k % 3) do s = s + x end return s end)
  e = e + (ok and 1 or #msg)
end
print(e)
local function arith() local i = 0 return function() i = i + 1 if i < 30 then return i end return {} + 1 end end
for k = 1, 5 do print(pcall(function() local s = 0 for x in arith() do s = s + x end return s end)) end
-- the iterator changes type of its value after many iterations (a type-guard exit)
local function drift(n) local i = 0 return function() i = i + 1 if i <= n then if i > 150 then return "s" .. i end return i end end end
local d = 0
for k = 1, 5 do for v in drift(180) do d = d + #tostring(v) end end
print(d)
-- upvalue written by the loop body and read by the iterator
local lim = 10
local function lazy() local i = 0 return function() i = i + 1 if i <= lim then return i end end end
local z = 0
for k = 1, 30 do lim = 10 + k % 5 for x in lazy() do z = z + x; if x == 3 then lim = lim - 1 end end end
print(z)
-- a closing value (4th value), a break out of the loop
local closed = 0
local function closer() return setmetatable({}, {__close = function() closed = closed + 1 end}) end
local cl = 0
for k = 1, 30 do
  for x in range(10), nil, nil, closer() do cl = cl + x; if x == 4 then break end end
end
print(cl, closed)
-- coroutine.wrap iterators (not recorded: a C function)
local function gen(n) return coroutine.wrap(function() for i = 1, n do coroutine.yield(i) end end) end
local w = 0
for k = 1, 30 do for x in gen(8) do w = w + x end end
print(w)
-- an iterator that is a callable table (`__call`), and a vararg iterator
local callable = setmetatable({}, {__call = function(self, s, c) c = c + 1 if c <= 5 then return c end end})
local cc = 0
for k = 1, 30 do for x in callable, nil, 0 do cc = cc + x end end
print(cc)
local function va(...) local n = select('#', ...) local i = 0 return function(...) i = i + 1 if i <= n then return i, ... end end end
local vv = 0
for k = 1, 30 do for a, b in va(1, 2, 3, 4) do vv = vv + a + (b or 0) end end
print(vv)
"#,
    },
    Prog {
        name: "tfor_pairs",
        small: true,
        src: r#"
-- pairs over arrays, hash tables and mixed tables: key and value types of every kind, one variable, deletion of the
-- current key during the traversal, a table that is modified (existing keys only) while it is traversed
local function sumv(t) local s = 0 for _, v in pairs(t) do s = s + v end return s end
local function sumk(t) local s = 0 for k in pairs(t) do s = s + k end return s end
local function cnt(t) local n = 0 for k, v in pairs(t) do n = n + 1 end return n end
local function strk(t) local s = 0 for k, v in pairs(t) do s = s + #k + v end return s end
local function vtype(t) local n = 0 for k, v in next, t do if type(v) == "string" then n = n + #v elseif type(v) == "table" then n = n + 1 else n = n + 100 end end return n end
local function clear(t) for k in pairs(t) do t[k] = nil end end
local function bump(t) for k, v in pairs(t) do t[k] = v + 1 end end
local function kinds(t) local n = 0 for k, v in pairs(t) do local tk = type(k) if tk == "number" then n = n + 1 elseif tk == "string" then n = n + 2 elseif tk == "boolean" then n = n + 4 else n = n + 8 end end return n end
local arr, hash, mix = {}, {}, {}
for i = 1, 150 do arr[i] = i end
for i = 1, 150 do hash["k" .. i] = i * 2 end
for i = 1, 60 do mix[i] = i; mix["m" .. i] = i + 0.5 end
mix[100] = 1; mix[2.5] = 2; mix[true] = 3
local r = 0
for round = 1, 30 do
  r = r + sumv(arr) + sumk(arr) + cnt(arr) + sumv(hash) + strk(hash) + sumv(mix) * 2 + cnt(mix)
  r = r + kinds(mix) + vtype({1, "abc", {}, 2.5, true, x = "q", y = {}})
  bump(arr); bump(hash)
end
print(r)
local tmp = {}
for round = 1, 20 do
  for i = 1, 40 do tmp[i] = i; tmp["s" .. i] = i end
  clear(tmp)
  r = r + cnt(tmp)
end
print(r, next({}), cnt({}), cnt({1}))
local tk = {}
-- (every value is the same: the traversal order of table keys follows their ADDRESSES, which differ between a traced and an
-- interpreter run; an order-dependent running sum would differ at the oracle's checkpoints without any bug)
for i = 1, 30 do tk[{}] = 3 end
local n = 0
for round = 1, 20 do for k, v in pairs(tk) do n = n + v + (type(k) == "table" and 1 or 0) end end
print(n)
local fl = {}
for i = 1, 50 do fl[i + 0.5] = i; fl[-i] = i end
local m = 0
for round = 1, 20 do for k, v in pairs(fl) do m = m + v + (k > 0 and 1 or 2) end end
print(m)
"#,
    },
    Prog {
        name: "tfor_errors",
        small: true,
        src: r#"
-- a __pairs metamethod, a table with a metatable, nested loops and breaks, and (Q5) an ipairs over a table whose Lua
-- `__index` runs inside the iterator, combined with the traced pairs loops
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  local t = {}
  for i = 1, 20 do t[i] = i; t["x" .. i] = i end
  r = r + walk(t, function(t, k) t[k] = nil end)
  local u = setmetatable({}, {__pairs = function(t) return function(_, k) if (k or 0) < 5 then return (k or 0) + 1, 7 end end, t, nil end})
  r = r + walk(u)
  local w = setmetatable({a = 1, b = 2, c = 3}, {__index = function() return 0 end})
  r = r + walk(w)
  for i = 1, 3 do for k, v in pairs({10, 20, 30}) do r = r + k * v; if k == 2 then break end end end
  for i, v in ipairs(setmetatable({}, {__index = function(_, i) if i < 4 then return i end end})) do r = r + v end
end
print(r)
print(pcall(next, {}, "nokey"))
"#,
    },
    Prog {
        name: "tfor_te1",
        small: true,
        src: r#"
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  local t = {}
  for i = 1, 20 do t[i] = i; t["x" .. i] = i end
  r = r + walk(t, function(t, k) t[k] = nil end)
end
print(r)
"#,
    },
    Prog {
        name: "tfor_te2",
        small: true,
        src: r#"
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  local u = setmetatable({}, {__pairs = function(t) return function(_, k) if (k or 0) < 5 then return (k or 0) + 1, 7 end end, t, nil end})
  r = r + walk(u)
end
print(r)
"#,
    },
    Prog {
        name: "tfor_te3",
        small: true,
        src: r#"
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  local w = setmetatable({a = 1, b = 2, c = 3}, {__index = function() return 0 end})
  r = r + walk(w)
end
print(r)
"#,
    },
    Prog {
        name: "tfor_te4",
        small: true,
        src: r#"
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  for i, v in ipairs(setmetatable({}, {__index = function(_, i) if i < 4 then return i end end})) do r = r + v end
end
print(r)
"#,
    },
    Prog {
        name: "tfor_te5",
        small: true,
        src: r#"
local function walk(t, f) local n = 0 for k, v in pairs(t) do n = n + 1; if f then f(t, k) end end return n end
local r = 0
for round = 1, 40 do
  for i = 1, 3 do for k, v in pairs({10, 20, 30}) do r = r + k * v; if k == 2 then break end end end
end
print(r)
"#,
    },
    Prog {
        name: "tfor_te6",
        small: true,
        src: r#"
-- adding keys while traversing: the control key may be gone at the next call ("invalid key to 'next'") or not
local out = {}
for round = 1, 60 do
  local t = {}
  for i = 1, 100 do t["k" .. i] = i end
  local seen = 0
  local ok, e = pcall(function()
    for k in pairs(t) do
      seen = seen + 1
      if seen == 60 then t[k] = nil; for j = 1, 300 do t["n" .. j] = j end end
    end
  end)
  out[#out + 1] = tostring(ok) .. ":" .. tostring(e)
end
local seen = {}
for _, v in ipairs(out) do seen[v] = (seen[v] or 0) + 1 end
local ks = {}
for k, v in pairs(seen) do ks[#ks + 1] = k .. "=" .. v end
table.sort(ks)
print(table.concat(ks, ";"))
"#,
    },
    Prog {
        name: "tfor_te7",
        small: true,
        src: r#"
print(pcall(next, {}, "nokey"))
"#,
    },
    Prog {
        name: "str_concat",
        small: true,
        src: r#"
-- CONCAT of strings and numbers (two and three operands): integer and float conversions, results used as table keys
-- (interning), the short/long boundary at 40 bytes, empty operands, accumulation, and results compared by identity
local function c2(a, b) return a .. b end
local function c3(a, b, c) return a .. b .. c end
local function loop2(n, x) local t = {} for i = 1, n do t[#t + 1] = "k" .. i .. x end return t end
local function keys(n) local t = {} for i = 1, n do t["id" .. (i % 7)] = (t["id" .. (i % 7)] or 0) + i end return t end
local function acc(n) local s = "" for i = 1, n do s = s .. "x" end return #s end
local function acc2(n) local s = "ab" for i = 1, n do s = s .. i .. "," end return #s, s:sub(1, 20) end
local function mix(n) local r = 0 for i = 1, n do local f = i / 4; local s = f .. "|" .. i; r = r + #s end return r end
local function same(n) local c = 0 for i = 1, n do if ("a" .. i) == ("a" .. i) then c = c + 1 end end return c end
local function emp(n, e) local r = 0 for i = 1, n do local s = e .. "z"; r = r + #s; s = i .. e; r = r + #s end return r end
local function floats(n)
  local vals = {1.5, 2.0, -0.0, 1e100, 0.1, 1/0, -1/0, 2^53, 123456789012345.0}
  local r = 0
  for i = 1, n do local v = vals[i % #vals + 1]; local s = "v=" .. v; r = r + #s end
  return r
end
local function bound(n, w) local r = 0 for i = 1, n do local s = w .. "0123456789" .. i; r = r + #s end return r end
local t = loop2(150, "!")
local k = keys(200)
local sum = 0
for key, v in pairs(k) do sum = sum + v + #key end
print(#t, t[1], t[150], sum, acc(300), acc2(60))
print(mix(300), same(200), emp(100, ""), emp(100, "q"), floats(180))
print(bound(100, string.rep("a", 17)), bound(100, string.rep("b", 18)), bound(100, string.rep("c", 40)))
local r = 0
for round = 1, 40 do r = r + #c2("ab", round) + #c3(round, ".", round * 2) + #c2(round + 0.5, "x") + #c2("p", round % 3 == 0 and "" or "q") end
print(r)
print(pcall(c2, {}, "x"), pcall(c3, "a", nil, "b"))
print(pcall(function() local s = setmetatable({}, {__concat = function(a, b) return "mm" end}); local o = 0 for i = 1, 100 do o = o + #(s .. i) + #(i .. s) end return o end))
"#,
    },
    Prog {
        name: "strto_for",
        small: true,
        src: r#"
-- integer `for` loops whose limit is a numeral string (coerced by `forlimit`), ascending and descending, fractional
-- and hexadecimal numerals, a limit that is no numeral (error), nested in a hot loop so the inner FORPREP is recorded
local function f(n, lim) local s = 0 for i = 1, n do for j = 1, lim do s = s + j end end return s end
local function g(n, lim) local s = 0 for i = 1, n do for j = 10, lim, -1 do s = s + j end end return s end
local function h(n, lim, st) local s = 0 for i = 1, n do for j = 1, lim, st do s = s + j end end return s end
local r = 0
for round = 1, 20 do
  r = r + f(30, "7") + f(30, "7.9") + f(30, "0x5") + f(30, " 12 ") + g(30, "7.2") + g(30, "3") + h(30, "20", 3) + h(30, 9, 2)
end
print(r)
print(pcall(f, 30, "abc"))
print(pcall(f, 30, "-1e100"))
"#,
    },
    Prog {
        name: "str_temporaries",
        small: true,
        src: r#"
-- the registers CONCAT leaves behind (the converted operands) are visible through debug.getlocal: after the hot loop ends
-- (a trace exit) every temporary holds what the interpreter left there
local function f(n)
  local s = ""
  for i = 1, n do s = "k" .. i .. 1.5 end
  local t = {}
  for k = 1, 16 do
    local name, v = debug.getlocal(1, k)
    t[#t + 1] = (name or "-") .. ":" .. type(v) .. (type(v) == "string" and v or "")
  end
  return s, table.concat(t, " ")
end
for round = 1, 30 do local a, b = f(20 + round) if round == 30 then print(a, b) end end
print(f(3))
"#,
    },
    Prog {
        name: "tfor_iter_swap",
        small: true,
        src: r#"
-- one loop site, different iterators over time: next, the ipairs iterator, a Lua iterator
local function sum(it, t, c) local s = 0 for k, v in it, t, c do s = s + (type(k) == "number" and k or 0) + v end return s end
local a = {} for i = 1, 40 do a[i] = i end
local h = {} for i = 1, 40 do h["k" .. i] = i end
local function lua_it(t, i) i = i + 1 if t[i] then return i, t[i] end end
local r = 0
for round = 1, 40 do
  r = r + sum(next, a) + sum(ipairs(a)) + sum(next, h) + sum(lua_it, a, 0)
  if round % 7 == 0 then r = r + sum(ipairs(h)) + sum(pairs(a)) end
end
print(r)
"#,
    },
    // ---- F4: recursion unroll, TAILCALL, multiple results with an explicit `top` ----
    Prog {
        name: "rec_fib",
        small: true,
        src: r#"
-- recursion inside a hot loop: unrolled up to the limit, the recording ends (or aborts) beyond it
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function run(n, reps) local s = 0 for i = 1, reps do s = s + fib(n) end return s end
print(run(2, 120), run(3, 100), run(5, 40), run(9, 12), fib(14), run(1, 50), run(0, 50))
"#,
    },
    Prog {
        name: "rec_ack",
        small: true,
        src: r#"
-- ackermann: recursive calls, tail calls and a recursive argument in one function
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
local function run(m, n, reps) local s = 0 for i = 1, reps do s = s + ack(m, n) end return s end
print(run(1, 2, 60), run(2, 2, 40), run(2, 3, 20), run(0, 5, 50), run(3, 2, 6), run(1, 2, 60))
"#,
    },
    Prog {
        name: "rec_depth",
        small: true,
        src: r#"
-- the recursion depth changes between phases: traces recorded at one depth exit at another (beyond the unroll limit, the
-- frames of the recursion must be rebuilt exactly)
local function depth(n) if n == 0 then return 0 end return 1 + depth(n - 1) end
local function run(d, reps) local s = 0 for i = 1, reps do s = s + depth(d) end return s end
print(run(1, 80), run(2, 80), run(7, 80), run(2, 80), run(30, 12), run(0, 80), run(3, 80), run(40, 5))
"#,
    },
    Prog {
        name: "rec_depth_locals",
        small: true,
        src: r#"
-- registers of popped recursive frames are visible through debug.getlocal after the loop's exit
local function depth(n, a, b) local c = a + n if n == 0 then return c end local d = depth(n - 1, c, b) return d + b end
local function run(d, reps)
  local s = 0
  for i = 1, reps do s = s + depth(d, i, 2) end
  local t = {}
  for k = 1, 14 do local name, v = debug.getlocal(1, k) t[#t + 1] = (name or "-") .. ":" .. (type(v) == "table" and "tab" or tostring(v)) end
  return s, table.concat(t, " ")
end
print(run(2, 90))
print(run(1, 90))
print(run(5, 90))
"#,
    },
    Prog {
        name: "rec_mutual",
        small: true,
        src: r#"
local isodd
local function iseven(n) if n == 0 then return true end return isodd(n - 1) end
function isodd(n) if n == 0 then return false end return iseven(n - 1) end
local function run(n, reps) local c = 0 for i = 1, reps do if iseven(n + i % 2) then c = c + 1 end end return c end
print(run(4, 100), run(1, 100), run(9, 60), run(0, 100), run(4, 100))
"#,
    },
    Prog {
        name: "rec_tree",
        small: true,
        src: r#"
-- a recursive tree walk with a loop in the caller (binary_trees shape, no allocation inside the loop)
local function build(d) if d == 0 then return {} end return {build(d - 1), build(d - 1)} end
local function count(t) local l = t[1] if l then return 1 + count(l) + count(t[2]) end return 1 end
local function run(t, reps) local s = 0 for i = 1, reps do s = s + count(t) end return s end
local t2, t4 = build(2), build(4)
print(run(t2, 90), run(t4, 40), run(t2, 90), run(build(0), 90))
"#,
    },
    Prog {
        name: "rec_error",
        small: true,
        src: r#"
-- an error raised in the middle of a recursion that is running on a trace: the traceback shows every recursive frame
local function g(n, t) if n == 0 then return t.x + 1 end return 1 + g(n - 1, t) end
local function run(reps, bad)
  local ts = {}
  for i = 1, reps do ts[i] = {x = i} end
  ts[bad] = 5
  local s = 0
  for i = 1, reps do s = s + g(i % 2, ts[i]) end
  return s
end
print(pcall(run, 100, 1000))
print(pcall(run, 100, 80))
print(select(2, xpcall(run, debug.traceback, 100, 97)))
"#,
    },
    Prog {
        name: "rec_error_deep",
        small: true,
        src: r#"
-- the error comes from the arithmetic of the third nesting level (beyond the unroll limit when unroll = 2)
local function h(n, x) if n == 0 then return x + 1 end return 1 + h(n - 1, x) end
local function run(reps, bad)
  local xs = {}
  for i = 1, reps do xs[i] = i end
  xs[bad] = "z"
  local s = 0
  for i = 1, reps do s = s + h(3, xs[i]) end
  return s
end
print(pcall(run, 100, 1000))
print(pcall(run, 100, 70))
"#,
    },
    Prog {
        name: "rec_coroutine",
        small: true,
        src: r#"
-- a coroutine yields from inside a recursion that its hot loop drives
local function walk(n) if n == 0 then coroutine.yield(1) return 0 end return 1 + walk(n - 1) end
local co = coroutine.wrap(function()
  local s = 0
  for i = 1, 70 do s = s + walk(i % 3) end
  return s
end)
local acc = 0
for k = 1, 71 do acc = acc + co() end
print(acc)
"#,
    },
    Prog {
        name: "rec_coroutine_tail",
        small: true,
        src: r#"
-- yields from a tail-called frame, in a loop with a recursive call before the yield
local function leaf(n) if n % 4 == 0 then coroutine.yield(n) end return n end
local function mid(n) return leaf(n) end
local function rec(n) if n <= 0 then return mid(n + 4) end return rec(n - 1) + 1 end
local co = coroutine.wrap(function()
  local s = 0
  for i = 1, 60 do s = s + rec(i % 2) end
  return s
end)
local acc = 0
for k = 1, 31 do acc = acc * 3 % 1000003 + co() end
print(acc)
"#,
    },
    Prog {
        name: "s1_frame_sides",
        small: true,
        src: r##"
-- S1: hot guard exits inside inlined frames get side traces that keep the frames virtual (depth 1..4, tail frames, closures of
-- one prototype with different upvalues, result adjustments, errors and tracebacks, coroutines, stack growth, hooks).
local N = 2600
local sum = 0
-- (1) a polymorphic branch in an inlined helper, three ways, depth 1
local function sel(x)
  local m = x % 3
  if m == 0 then return x + 1 elseif m == 1 then return x * 2 else return x - 1 end
end
local s1 = 0
for i = 1, N do s1 = s1 + sel(i) end
sum = sum + s1
-- (2) depth 2, 3, 4: the branch in the innermost frame, in a middle frame after the return, and in the root after the call
local function d4(x) if x % 2 == 0 then return x // 2 end return 3 * x + 1 end
local function d3(x) local v = d4(x) if v % 5 == 0 then return v - 1 end return v end
local function d2(x) local v = d3(x + 1) + d3(x) if x % 7 == 0 then return v * 2 end return v end
local function d1(x) return d2(x) + 1 end
local s2 = 0
for i = 1, N do
  local r = d1(i)
  if r % 3 == 0 then s2 = s2 + r else s2 = s2 - 1 end
end
sum = sum + s2
-- (3) tail calls: parity of the chain length alternates (the frame is replaced), mutual and self tail recursion
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local function lp(n, acc) if n == 0 then return acc end return lp(n - 1, acc + n) end
local s3 = 0
for i = 1, N do
  if ev(6 + i % 2) then s3 = s3 + 1 end
  s3 = s3 + lp(i % 5 + 2, 0)
end
sum = sum + s3
-- (4) closures of one prototype with different upvalues called alternately, an upvalue written in the helper
local function mk(k)
  local acc = 0
  return function(x) acc = acc + 1 if x % 2 == 0 then return x + k + acc % 3 end return x - k end
end
local fs = {mk(1), mk(10), mk(100)}
local s4 = 0
for i = 1, N do s4 = s4 + fs[i % 3 + 1](i) end
sum = sum + s4
-- (5) result adjustment: 0, 1, 2 and open results, different counts per branch
local function r2(x) if x % 3 == 0 then return x, x + 1 elseif x % 3 == 1 then return x else return end end
local function r3(x) return r2(x) end
local s5 = 0
for i = 1, N do
  local a, b = r2(i)
  r2(i)
  local c = r2(i)
  local t = {r3(i)}
  s5 = s5 + (a or 0) + (b or 100) + (c or 7) + #t
  s5 = s5 + select("#", r3(i))
end
sum = sum + s5
-- (6) an error raised in the rare branch of an inlined helper, with the traceback of the frames
local function thrower(x) if x % 97 == 0 then error("boom " .. x) end return x end
local function mid(x) local v = thrower(x) return v + 1 end
local s6, errs, tb = 0, 0, 0
for i = 1, N do
  local ok, v = xpcall(mid, function(m) tb = tb + #debug.traceback(m, 1) return m end, i)
  if ok then s6 = s6 + v else errs = errs + 1 end
end
sum = sum + s6 + errs * 1000 + tb % 1000
-- (7) a rare branch that recurses deeply (the stack grows while the frames are virtual)
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
local function grow(x) if x % 400 == 399 then return deep(180) end return x % 5 end
local s7 = 0
for i = 1, N do s7 = s7 + grow(i) end
sum = sum + s7
-- (8) a coroutine whose loop runs a polymorphic helper and yields from the loop body
local co = coroutine.wrap(function()
  local t = 0
  for i = 1, N do
    t = t + sel(i) + d1(i % 11)
    if i % 500 == 0 then coroutine.yield(t) end
  end
  return t
end)
local s8 = 0
for k = 1, 6 do s8 = s8 + (co() or 0) % 100003 end
sum = sum + s8
-- (9) eight-way branching in one helper (several side traces off one frame)
local function oct(x)
  local m = x & 7
  if m == 0 then return 1 elseif m == 1 then return 2 elseif m == 2 then return 4 elseif m == 3 then return 8
  elseif m == 4 then return 16 elseif m == 5 then return 32 elseif m == 6 then return 64 end
  return 128
end
local s9 = 0
for i = 1, N * 2 do s9 = s9 + oct(i) end
sum = sum + s9
-- (10) a hook installed in the middle of the loop, then removed
local cnt = 0
local s10 = 0
for i = 1, N do
  if i == 1200 then debug.sethook(function() cnt = cnt + 1 end, "", 50) end
  if i == 1800 then debug.sethook() end
  s10 = s10 + sel(i) + oct(i)
end
sum = sum + s10 + (cnt > 0 and 1 or 0)
-- (11) a type change of the helper's argument (a type-guard exit inside the frame), floats and integers
local function poly(x) if x == 0 then return 0 end return x + 1 end
local s11 = 0
for i = 1, N do
  local v = (i % 4 == 0) and i + 0.5 or i
  s11 = s11 + poly(v)
end
sum = sum + s11
-- (12) varargs and a metamethod between the frames (kept out of virtual side traces, the results must not change)
local function va(...) local a, b = ... if a % 2 == 0 then return b end return a end
local mt = setmetatable({}, {__index = function(t, k) if k % 3 == 0 then return k else return -k end end})
local s12 = 0
for i = 1, N do s12 = s12 + va(i, i * 2) + mt[i] + sel(mt[i + 1]) end
sum = sum + s12
-- (13) branchy metamethod bodies (frames with CIST_MMRET): the branch alternates with the iteration
local V = {}
V.__add = function(a, b) if a.x % 2 == 0 then return a.x + b.x else return a.x - b.x end end
V.__unm = function(a) if a.x % 3 == 0 then return -a.x end return a.x end
V.__len = function(a) if a.x % 2 == 0 then return 1 end return 2 end
V.__concat = function(a, b) if a.x % 2 == 0 then return 1 else return 2 end end
V.__eq = function(a, b) if a.x % 2 == 0 then return a.x == b.x end return false end
V.__lt = function(a, b) if a.x % 3 == 0 then return a.x < b.x end return a.x + 1 == b.x end
V.__le = function(a, b) if a.x % 5 == 0 then return true end return a.x <= b.x end
V.__call = function(self, k) if k % 2 == 0 then return self.x + k end return self.x end
local mA, mB = setmetatable({x = 1}, V), setmetatable({x = 2}, V)
local IX = setmetatable({}, {__index = function(t, k) if k % 2 == 0 then return k end return -k end})
local NI = 0
local WI = setmetatable({}, {__newindex = function(t, k, v) if k % 2 == 0 then NI = NI + v else NI = NI - 1 end end})
local s13 = 0
for i = 1, N do
  mA.x = i
  s13 = s13 + (mA + mB) + (-mA)
end
for i = 1, N do
  mA.x = i
  if mA < mB then s13 = s13 + 1 end
  if mA <= mB then s13 = s13 + 2 end
end
for i = 1, N do
  mA.x = i
  if mA == mB then s13 = s13 + 4 end
  s13 = s13 + #mA
end
for i = 1, N do
  mA.x = i
  s13 = s13 + (mA .. mB)
end
for i = 1, N do
  mA.x = i
  s13 = s13 + mA(i) + IX[i] + IX[i % 7 + 1]
  WI[i] = i
end
sum = sum + s13 + NI
-- (3b) tail frames are visible to the traceback of a frame that is rebuilt by an exit (CIST_TAIL of the rebuilt frames)
local te, to
function te(n) if n == 0 then return #debug.traceback("x", 1) end return to(n - 1) end
function to(n) if n == 0 then return #debug.getinfo(1, "S").short_src % 7 + (debug.getinfo(1, "t").istailcall and 1000 or 0) end return te(n - 1) end
local s3b = 0
for i = 1, N do s3b = s3b + te(2 + i % 2) + to(i % 3) end
sum = sum + s3b
-- (3c) a second alternation inside a tail frame that already is a side trace's start: the exits of the side rebuild the frame (CIST_TAIL)
local function tl(x)
  if x % 2 == 0 then
    if x % 3 == 0 then return #debug.traceback("y", 1) + (debug.getinfo(1, "t").istailcall and 7 or 0) end
    return 1
  end
  return 2
end
local function tc(x) return tl(x) end
local s3c = 0
for i = 1, N do s3c = s3c + tc(i) end
sum = sum + s3c
-- (14) fresh coroutines (small stacks): a side trace that needs deeper frames and more registers than its parent saw
local function c4(x) if x % 3 == 0 then return x + 4 end return x - 4 end
local function c3(x) if x % 2 == 0 then return c4(x) + 3 end return c4(x + 1) - 3 end
local function c2(x) return c3(x) + c3(x + 1) end
local function c1(x) local v = c2(x) if v % 5 == 0 then return v + 1 end return v end
local bigsrc = {"return function(x)"}
for i = 1, 120 do bigsrc[#bigsrc + 1] = "local v" .. i .. " = x + " .. i end
bigsrc[#bigsrc + 1] = "return v1 + v60 + v120 end"
local big = assert(load(table.concat(bigsrc, "\n")))()
local function pick(x) if x % 2 == 0 then return big(x) end return x end
local s14 = 0
for r = 1, 30 do
  local co = coroutine.wrap(function()
    local t = 0
    for i = 1, 400 do t = t + pick(i) + c1(i) end
    return t
  end)
  s14 = s14 + co()
  if r % 5 == 0 then collectgarbage() end
end
sum = sum + s14 + s3b
print(sum, s1, s2, s3, s4, s5, s6, errs, s7, s8, s9, s10, s11, s12, s13, NI, s3b, s14, s3c)
"##,
    },
    Prog {
        name: "tail_loop",
        small: true,
        src: r#"
-- a tail-recursive loop: the frame is replaced, not pushed
local function loop(n, acc) if n == 0 then return acc end return loop(n - 1, acc + n) end
local function run(m, reps) local s = 0 for i = 1, reps do s = s + loop(m, 0) end return s end
print(run(10, 100), run(3, 100), run(25, 60), run(0, 50), run(10, 100), run(1, 100))
"#,
    },
    Prog {
        name: "tail_args",
        small: true,
        src: r#"
-- tail calls with fewer / more arguments than parameters, different callees, multiple results
local function f(a, b, c) if a == 0 then return b, c end return f(a - 1, b) end
local function g(a, b) return f(a, b, a) end
local function h(a) return g(a, a * 2) end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local x, y = h(i % 4)
    s = s + (x or 0) + (y or -1)
    local p, q, r = g(i % 3, 1)
    s = s + (p or 0) * 3 + (q or 0) * 5 + (r or 0) * 7
  end
  return s
end
print(run(120), run(7))
"#,
    },
    Prog {
        name: "tail_mutual",
        small: true,
        src: r#"
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
local function run(n, reps) local c = 0 for i = 1, reps do if even(n + i % 2) then c = c + 1 end end return c end
print(run(6, 100), run(1, 100), run(0, 100), run(12, 50), run(6, 100))
"#,
    },
    Prog {
        name: "tail_flag",
        small: true,
        src: r#"
-- CIST_TAIL of a tail-called frame survives an exit (debug.getinfo / traceback see "(...tail calls...)")
local function leaf(n)
  if n % 5 == 0 then
    local t = debug.getinfo(1, "t").istailcall
    local tb = debug.traceback("x", 1)
    return (t and 1 or 0) + #tb
  end
  return n
end
local function mid(n) return leaf(n) end
local function top(n) return mid(n) end
local function run(reps) local s = 0 for i = 1, reps do s = s + top(i) end return s end
print(run(200))
print(run(7))
"#,
    },
    Prog {
        name: "tail_locals",
        small: true,
        src: r#"
-- the registers a tail call leaves behind (the replaced frame's locals above the new arguments) at an exit
local function leaf(n) local a, b, c = n, n + 1, n + 2 if n > 90 then return debug.getlocal(1, 3) end return a + b + c end
local function mid(n) local x, y, z, w = 1, 2, 3, 4 return leaf(n + x + y + z + w - 10) end
local function run(reps)
  local s = 0
  for i = 1, reps do local a, b = mid(i) s = s + (type(a) == "string" and #a or a) + (b or 0) end
  return s
end
print(run(100))
"#,
    },
    Prog {
        name: "swap_divmod",
        small: true,
        src: r#"
local function swap(a, b) return b, a end
local function divmod(a, b) return a // b, a % b end
local function run(reps)
  local s, x, y = 0, 1, 2
  for i = 1, reps do
    x, y = swap(x, y)
    local q, r = divmod(i * 7 + x, 5 + y)
    s = s + q * 3 + r + x
  end
  return s, x, y
end
print(run(150))
print(run(3))
"#,
    },
    Prog {
        name: "multret_calls",
        small: true,
        src: r#"
-- C = 0 calls feeding B = 0 calls and returns
local function two(a) return a, a + 1 end
local function three(a) return a, a + 1, a + 2 end
local function none() end
local function sum(a, b, c) return a + b + (c or 0) end
local function wrap(a) return 0, two(a) end
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + sum(two(i))
    s = s + sum(three(i))
    s = s + sum(1, two(i))
    s = s + sum(wrap(i))
    s = s + sum(none() or 1, 5, 6)
    local p, q, r = wrap(i)
    s = s + p + q + r
    local u, v = three(i)
    s = s + u * v
  end
  return s
end
print(run(120), run(5))
"#,
    },
    Prog {
        name: "multret_return",
        small: true,
        src: r#"
-- RETURN B = 0 nested several frames deep, and the wanted count of the outer call decides how many arrive
local function two(a) return a, a + 1 end
local function r1(a) return two(a) end
local function r2(a) return 7, r1(a) end
local function r3(a) return r2(a) end
local function sum(a, b, c, d) return a + b + (c or 0) + (d or 0) end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local a = r3(i)
    local b, c = r3(i)
    local d, e, f, g = r3(i)
    s = s + a + b + c + d + e + f + (g or 0)
    s = s + sum(r3(i))
  end
  return s
end
print(run(100), run(4))
"#,
    },
    Prog {
        name: "multret_intrinsic",
        small: true,
        src: r#"
-- an intrinsic fed by (and feeding) a multiple-result sequence
local function two(a) return a, 3 end
local function pair(a) return a * 1.5, 2.5 end
local function pick(a, b) return math.max(a, b) end
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + math.max(two(i))
    s = s + math.min(two(i))
    s = s + math.max(pair(i))
    s = s + pick(two(i))
    s = s + math.floor(i / 3)
  end
  return s
end
print(run(150))
"#,
    },
    Prog {
        name: "multret_cfunc",
        small: true,
        src: r#"
-- C functions with B = 0 / C = 0 (stitching or abort): table.pack, table.unpack, string.byte (select is inlined since P-E)
local function two(a) return a, a + 1 end
local function run(reps)
  local s = 0
  local t = {1, 2, 3}
  for i = 1, reps do
    s = s + table.pack(two(i)).n
    s = s + (select('#', two(i)))
    s = s + select(2, two(i))
    local a, b, c = table.unpack(t)
    s = s + a + b + c
    s = s + string.byte("hello", 1, 3)
  end
  return s
end
print(run(130))
"#,
    },
    Prog {
        name: "vararg_select",
        small: true,
        src: r#"
-- vararg functions (not inlined): select('#', ...) and friends keep working next to traced code
local function cnt(...) return select('#', ...) end
local function first(...) local a = ... return a end
local function pack2(...) return ... end
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + cnt() + cnt(i) + cnt(i, i, i) + cnt(pack2(i, 2))
    s = s + first(i, 5) + (select(2, pack2(i, 4, 5)))
    local t = {pack2(1, 2, 3)}
    s = s + #t
  end
  return s
end
print(run(120))
"#,
    },
    Prog {
        name: "multret_setlist",
        small: true,
        src: r#"
-- a constructor ending in a call (SETLIST up to the top): not recorded, runs in the interpreter / tier 1
local function two(a) return a, a + 1 end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local t = {i, two(i)}
    s = s + #t + t[3]
    local u = {two(i)}
    s = s + u[2]
  end
  return s
end
print(run(120))
"#,
    },
    Prog {
        name: "multret_vararg_root",
        small: true,
        src: r#"
-- a loop inside a vararg function calling a function with multiple results
local function two(a) return a, a + 1 end
local function f(...)
  local s = select('#', ...)
  for i = 1, 100 do local a, b = two(i) s = s + a + b + (select(2, ...)) end
  return s, ...
end
print(f(1, 2, 3))
print(f(7, 8))
"#,
    },
    // ---- P-A: `__index` chains (instance without the key -> class tables), the guards of every level ----
    Prog {
        name: "oop_chain_scenarios",
        small: true,
        src: r#"
-- one hot loop reading methods and fields through a three-level `__index` chain; at iteration K the scenario changes
-- something a guard of the trace stands for: a shadowing own key (the absent-key proof: key at the main position, or a
-- link in a collision chain), a rehash of the instance, a class field or method replaced, the instance's metatable removed,
-- its `__index` turned into a function / removed, a nil-valued own key given a value, a table with no metatable that gets
-- an `__index` (the end of the chain)
local function classes()
  local A = { kA = 1 }; A.__index = A
  function A:get() return self.x end
  local B = setmetatable({ kB = 2 }, { __index = A }); B.__index = B
  local C = setmetatable({}, { __index = B }); C.__index = C
  return A, B, C
end
local SHADOW = { "get", "kA", "kB", "missing" }
-- every execution compiles its own copy of the function: the loop has a fresh trace head (no blacklisting between scenarios)
local SRC = [[
local classes, scn, nf, n, k, name = ...
local A, B, C = classes()
local o = setmetatable({ x = 1 }, C)
for j = 1, nf do o["p" .. j] = j end
o.nilk = 3; o.nilk = nil
local s = 0
for i = 1, n do
  s = s + o:get() + o.kA + o.kB + (o.missing or 5) + (o.nilk or 7)
  if i == k then
    MUT
  end
end
return s
]]
local MUTS = {
  "o[name] = function() return 100 end", "o[name] = function() return 100 end", "o[name] = function() return 100 end",
  "o[name] = function() return 100 end", "o[name] = 10", "o[name] = 10", "o[name] = 10", "o[name] = 10",
  "B.kB = 20", "A.get = function(self) return -self.x end", "setmetatable(o, nil)",
  "getmetatable(o).__index = function() return 1 end", "getmetatable(o).__index = nil", "o.nilk = 4",
  "setmetatable(A, { __index = { missing = 9 } })", "for j = 1, 20 do o['f' .. j] = j end", "C.missing = 6",
  "getmetatable(o).__index = A", "getmetatable(o).__index = 5", "B.__index = function() return 2 end",
}
local function run(scn, nf, n, k)
  local f = assert(load((SRC:gsub("MUT", function() return MUTS[scn + 1] end)), "scn", "t"))
  return f(classes, scn, nf, n, k, SHADOW[scn % 4 + 1])
end
for scn = 0, 19 do
  for _, nf in ipairs{ 0, 1, 2, 3, 4, 6 } do
    local ok, e = pcall(run, scn, nf, 40, 20)
    print(scn, nf, ok, e)
  end
end
"#,
    },
    Prog {
        name: "oop_chain_overrides",
        small: true,
        src: r#"
-- OOP programs: inheritance depth 1-3, a method overridden after warm-up, class mutation inside the loop, a base-class
-- default changed, instances of different classes alternating in one loop (the receiver's metatable varies)
local Base = {}; Base.__index = Base
Base.scale = 2
function Base.new(x) return setmetatable({ x = x, n = 0 }, Base) end
function Base:inc(d) self.x = self.x + d; self.n = self.n + 1 end
function Base:get() return self.x * self.scale end
local Mid = setmetatable({}, { __index = Base }); Mid.__index = Mid
function Mid.new(x) local o = Base.new(x); o.w = 3; return setmetatable(o, Mid) end
function Mid:inc(d) Base.inc(self, d); self.n = self.n + 1 end
local Leaf = setmetatable({}, { __index = Mid }); Leaf.__index = Leaf
function Leaf.new(x) local o = Mid.new(x); o.z = 1.5; return setmetatable(o, Leaf) end
local function f(objs, n, k)
  local s = 0
  for i = 1, n do
    local o = objs[i % #objs + 1]
    o:inc(i % 5)
    s = s + o:get() + (o.w or 0) + (o.z or 0)
    if i == k then Base.get = function(self) return self.x end end
    if i == k + 5 then Base.scale = 3 end
    if i == k + 10 then Leaf.inc = function(self, d) self.x = self.x - d end end
  end
  return s
end
local function same(ctor, n, k)
  local o = ctor(1)
  local s = 0
  for i = 1, n do o:inc(i % 7); s = s + o:get() + o.scale; if i == k then Mid.scale = 10 end end
  return s, o.x, o.n
end
print(pcall(f, { Base.new(1), Mid.new(2), Leaf.new(3), Base.new(4) }, 80, 30))
Base.get = function(self) return self.x * self.scale end; Base.scale = 2; Leaf.inc = nil; Mid.scale = nil
print(pcall(f, { Leaf.new(1) }, 80, 40))
Base.get = function(self) return self.x * self.scale end; Base.scale = 2; Leaf.inc = nil; Mid.scale = nil
print(pcall(same, Base.new, 80, 40))
Mid.scale = nil
print(pcall(same, Mid.new, 80, 40))
Mid.scale = nil
print(pcall(same, Leaf.new, 80, 40))
"#,
    },
    Prog {
        name: "vararg_fwd",
        small: true,
        src: r#"
-- P-E: vararg callees inlined: forwarding, counts, nil padding, fixed parameters before `...`
local function pass(...) return ... end
local function cnt(...) return select('#', ...) end
local function second(...) return (select(2, ...)) end
local function last(...) return (select(-1, ...)) end
local function tail2(...) return select(2, ...) end
local function fixed(a, b, ...) local c, d = ... return a, b, c, d end
local function mixed(a, ...) return a, ... end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local a, b, c = pass(i, i + 1, 3)
    s = s + a + b + c
    s = s + cnt() + cnt(nil) + cnt(i, i) + cnt(pass(i, i, i)) + cnt(pass())
    s = s + second(1, i, 3) + last(1, 2, i) + select('#', tail2(1, 2, 3, 4))
    local p, q, r, t = fixed(i, 1)
    s = s + p + q + (r or 0) + (t or 0)
    local u, v, w = mixed(i, 7, 8)
    s = s + u + v + w
    local x, y = fixed(i)
    s = s + x + (y == nil and 1 or 0)
    local m, n2 = fixed(i, 2, 3, 4, 5)
    s = s + m + n2
  end
  return s
end
print(run(140))
"#,
    },
    Prog {
        name: "vararg_loop",
        small: true,
        src: r#"
-- P-E: loops inside an inlined vararg callee are unrolled (constant `select('#', ...)`), `select(i, ...)` with a loop index
local function sum(...)
  local s = 0
  for i = 1, select('#', ...) do s = s + (select(i, ...)) end
  return s
end
local function sum2(...)
  local s = 0
  for i = 1, select('#', ...) do local v = select(i, ...) s = s + v end
  return s
end
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + sum(i, 2, 3, 4) + sum() + sum(i) + sum2(1.5, 2, i)
    if i % 11 == 0 then s = s + sum(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24) end
  end
  return s
end
print(run(140))
"#,
    },
    Prog {
        name: "vararg_types",
        small: true,
        src: r#"
-- P-E: a guard fails inside a vararg frame (the argument changes type late, the extra arguments change count): the exit rebuilds
-- the frame (`ci.func`, `nextraargs`) and tier 1 finishes the function and returns through the vararg epilogue
local reps_flip = 90
local function f(a, ...)
  local x = a + 1
  local n = select('#', ...)
  local v1, v2 = ...
  if v1 > reps_flip then x = x + 7 end
  return x + n + v1, v2
end
local function g(...)
  local a, b = ...
  return a * 2, b, ...
end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local a = i
    if i > reps - 40 then a = i + 0.5 end
    local r, v = f(a, i, 20, 30)
    s = s + r + v
    local x, y, z, w = g(i, 5, 6)
    s = s + x + y + z + w
    if i % 9 == 0 then
      local r2, v2 = f(a, i)
      s = s + r2 + (v2 == nil and 1 or 0)
    end
  end
  return s
end
print(run(150))
"#,
    },
    Prog {
        name: "vararg_nested",
        small: true,
        src: r#"
-- P-E: vararg callees calling vararg callees and ordinary functions, recursion through `...`, errors out of a vararg frame
local function pass(...) return ... end
local function twice(...) return pass(...), pass(...) end
local function inc(x) return x + 1 end
local function over(f, ...) return f(...) end
local function count(n, ...)
  if n == 0 then return select('#', ...) end
  return count(n - 1, n, ...)
end
local function boom(a, ...) return a + select(1, ...) end
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + select('#', twice(i, 2)) + over(inc, i) + over(pass, i, 2)
    s = s + (select(2, over(twice, 5, 6)))
    s = s + count(2, i)
    if i % 17 == 0 then
      local ok, e = pcall(boom, i, nil)
      s = s + (ok and 0 or 1)
      local ok2, e2 = pcall(boom, i)
      s = s + (ok2 and 0 or 1)
    end
  end
  return s
end
print(run(120))
"#,
    },
    Prog {
        name: "vararg_misc",
        small: true,
        src: r#"
-- P-E: shapes that stay aborts or take other paths: a vararg root loop, a vararg tail call, `{...}`, many arguments, and (5.5
-- only, through `load`) the vararg table
local function root(...)
  local s = select('#', ...)
  for i = 1, 100 do s = s + i + (select(1, ...)) end
  return s
end
local function pass(...) return ... end
local function tc(...) return pass(...) end
local function pack(...) local t = {...} return #t end
local ok, vt = pcall(load, "return function(...t) return t.n end")
local function run(reps)
  local s = 0
  for i = 1, reps do
    s = s + tc(i, 2) + pack(i, i)
    if ok and vt then s = s + vt()(i, i, i) end
  end
  return s
end
print(root(1, 2, 3), root(4), run(120))
"#,
    },
    Prog {
        name: "pack_basic",
        small: true,
        src: r#"
-- P-E2: `table.pack(...)` of a fixed argument count on a trace (the field `n` must be a constant of the function): counts,
-- nil arguments, collectable arguments, no arguments, the layout seen through `#`, `next` and `ipairs`
local function pv(...) local t = table.pack(...) return t.n, t[1], t[t.n] end
local function pn(...) local t = table.pack(...) return t.n, #t, t[2], t[3] end
local function p0() local t = table.pack() return t.n end
local function pc(a, b) local t = table.pack(a, {a}, "s" .. b, b) return t.n + #t[2] + #t[3] + t[1] end
local function ord(...) local t = table.pack(...) local s = 0 for k, v in pairs(t) do s = s + (type(k) == "number" and k or 100) end return s + t.n end
local function run(reps)
  local s = 0
  for i = 1, reps do
    local n, a, b = pv(i, i + 1, i * 2)
    s = s + n + a + b
    local m, l, x, y = pn(i, nil, nil)
    s = s + m + l + (x or 0) + (y or 0)
    s = s + p0() + pc(i, i + 2) + ord(i, 2.5, "z")
    local q = table.pack(i, i, i)
    s = s + q.n * #q + select('#', table.unpack(q, 1, q.n))
  end
  return s
end
print(run(150), run(40), pv(1, nil, 3), pn(nil, nil))
"#,
    },
    Prog {
        name: "fnroot_fib",
        small: true,
        src: r#"
-- P-E2: function-entry roots: plain and doubly recursive functions called from straight-line code and from a callback
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function cnt(n) if n == 0 then return 0 end return 1 + cnt(n - 1) end
local function sum3(a, b, c) return fib(a) + fib(b) + cnt(c) end
local t = {}
for i = 1, 14 do t[i] = fib(i) end
local s = 0
for _, v in ipairs(t) do s = s + v end
local r = {}
for i = 1, 8 do r[#r + 1] = sum3(i, 10 - i, i * 3) end
local u = {}
for i = 1, 30 do u[i] = (i * 7) % 13 end
table.sort(u, function(a, b) return fib(a % 6) + a < fib(b % 6) + b end)
print(s, table.concat(r, ","), table.concat(u, ","), fib(16), cnt(40))
"#,
    },
    // BUNDLE-K L1 (`lazyci`): observers at depth inside native self-recursion; the sources are shared with `scripts/lazyci-diff.sh`
    Prog {
        name: "bk_obs1",
        small: false,
        src: include_str!("../../../../tests/lazyci/bk_obs1.lua"),
    },
    Prog {
        name: "bk_obs2",
        small: false,
        src: include_str!("../../../../tests/lazyci/bk_obs2.lua"),
    },
    // BUNDLE-L (`aidx`, `bhoist`): observers and boundary cases of array loops; the sources are shared with `scripts/bl-diff.sh`
    Prog {
        name: "bl_obs1",
        small: false,
        src: include_str!("../../../../tests/bl/bl_obs1.lua"),
    },
    Prog {
        name: "bl_oob",
        small: false,
        src: include_str!("../../../../tests/bl/bl_oob.lua"),
    },
    Prog {
        name: "bl_off",
        small: true,
        src: r#"
-- BUNDLE-M: array loops whose index is `k + c` with c != 0 (the displacement of the complemented index of `aidx`): finite differences forward and
-- backward, a stencil, floats and integers, a table whose array part is shorter than the loop (the guards must leave), a loop from a high start
local function fd(x, n) local s = 0 for k = 1, n - 2 do s = s + x[k + 1] * 2 - x[k] + x[k + 2] end return s end
local function bk(x, n) local s = 0 for k = 3, n do s = s + x[k - 1] * 3 - x[k - 2] + x[k] end return s end
local function st(x, y, n) for k = 2, n - 1 do y[k] = x[k - 1] + x[k] * 2 + x[k + 1] end return y[n - 1] end
local h = 0
for n = 4, 40 do
  local xi, xf, y = {}, {}, {}
  for k = 1, n do xi[k] = k * k % 11; xf[k] = (k * k % 11) / 4; y[k] = 0 end
  for rep = 1, 4 do
    h = h + fd(xi, n) + bk(xi, n) + st(xi, y, n)
    h = h + fd(xf, n) * 4 + bk(xf, n) * 4 + st(xf, y, n) * 4
  end
end
print(h)
local short = {1, 2, 3, 4, 5}
local r = {}
for i = 1, 30 do r[#r + 1] = tostring(pcall(fd, short, 9)) .. tostring(select(2, pcall(bk, short, 9))) end
print(table.concat(r, ","))
local t = {}
for k = 1, 12 do t[k] = k end
for i = 1, 200 do h = h + fd(t, 12) + bk(t, 12) end
print(h)
"#,
    },
    Prog {
        name: "k_strself2",
        small: true,
        src: r##"
-- BUNDLE-M: a hot method-call site whose receiver is mostly a string but sometimes not a string: a number, a boolean, nil, a userdata with its own
-- metatable (io.stdout), a table. The inline string receiver of tier-1 `SELF` must check the receiver's type before it uses the string metatable
-- (mutant `strself_tag_skip`): `(42):sub(1, 1)` is an error ("attempt to index a number value"), `io.stdout:upper()` a call of a nil method.
local items = { "hello", "world!", 42, "x", true, "yy", io.stdout, "zzz", false, { sub = function(self, a) return "t" .. a end }, "q" }
local out, errs = {}, {}
for rep = 1, 40 do
  for i = 1, #items do
    local s = items[i]
    local ok, r = pcall(function() return s:sub(1, 2) .. s:upper() end)
    if ok then out[#out + 1] = r else errs[#errs + 1] = (tostring(r):gsub("^[^:]*:%d+: ", "")) end
  end
end
print(#out, #errs, out[1], out[#out], errs[1], errs[2], errs[#errs])
local nums = 0
for i = 1, 300 do
  local v = (i % 50 == 0) and i or "s" .. i
  local ok = pcall(function() return v:len() end)
  if not ok then nums = nums + 1 end
end
print(nums)
"##,
    },
    Prog {
        name: "t1i_floor2",
        small: true,
        src: r##"
-- BUNDLE-M: the inline `math.floor` of tier 1 on floats whose floor does not fit an integer (the mutant `t1i_floor_inexact` stores the converted
-- garbage instead of leaving for the library call, which returns the float): huge, infinite, NaN, the integer limits, negative fractions.
local floor = math.floor
local xs = { 0.5, -1.5, 3.0, 2.0 ^ 62, 2.0 ^ 63, -2.0 ^ 63, 1e300, -1e300, 1 / 0, -1 / 0, 0 / 0, 9007199254740993.0, 7, -7 }
local acc = {}
for rep = 1, 60 do
  for i = 1, #xs do
    local r = floor(xs[i])
    if rep == 60 then acc[#acc + 1] = tostring(r) .. ":" .. math.type(r) end
  end
end
print(table.concat(acc, " "))
local s = 0
for i = 1, 400 do
  local v = i % 9 == 0 and 1e19 or i / 4
  local r = floor(v)
  if math.type(r) == "integer" then s = s + r else s = s + 1 end
end
print(s)
"##,
    },
    Prog {
        name: "bl_mat",
        small: true,
        src: r#"
-- BUNDLE-L: matrix products and dot products over rows, hot enough to trace; sizes that change between calls, a ragged row, floats and integers
local function mk(n, m, f) local t = {} for i = 1, n do local r = {} for j = 1, m do r[j] = f(i, j) end t[i] = r end return t end
local function mm(a, b, n)
  local c = {}
  for i = 1, n do
    local ai, ci = a[i], {}
    for j = 1, n do
      local s = 0.0
      for k = 1, n do s = s + ai[k] * b[k][j] end
      ci[j] = s
    end
    c[i] = ci
  end
  return c
end
local function dot(x, y, n) local s = 0 for k = 1, n do s = s + x[k] * y[k] end return s end
local h = 0
for n = 3, 24 do
  local a, b = mk(n, n, function(i, j) return (i + j) / n end), mk(n, n, function(i, j) return (i * j % 7) / n end)
  local c = mm(a, b, n)
  local tr = 0
  for i = 1, n do tr = tr + c[i][i] end
  h = h + math.floor(tr * 1000)
end
local x, y = mk(1, 64, function(i, j) return j end)[1], mk(1, 64, function(i, j) return 2 * j end)[1]
for r = 1, 60 do h = h + dot(x, y, 64) + dot(x, y, 10) end
print(h, pcall(dot, x, y, 65))
print(pcall(mm, mk(5, 5, function() return 1 end), mk(5, 3, function() return 1 end), 5))
"#,
    },
    Prog {
        name: "pm3_rec",
        small: true,
        src: r#"
-- P-M3: native self-recursion: fib-shaped recursion with changing arguments, argument types per call, results consumed in
-- loops and callbacks, mutual recursion (not a self-call), a vararg recursion, an overflow inside pcall, an error from depth
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function tri(n) if n < 3 then return 1 end return tri(n - 1) + tri(n - 2) + tri(n - 3) end
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local function va(n, ...) if n == 0 then return select('#', ...) end return va(n - 1, n, ...) end
local function deep(n) return 1 + deep(n + 1) end
local function bad(n) if n == 0 then error("boom") end return bad(n - 1) + 1 end
-- the second self-call recurses (the continuation traces link back into the entry trace), to the stack limit
local function zz(n) if n < 0 then return 0 end local x = zz(-1); return x + zz(n + 1) end
-- live tables in the last registers across the self-call (the collector must see them through the pushed frames)
local function gl(n)
  if n == 0 then return 0 end
  local a, b, c = { n }, { n + 1 }, { n + 2 }
  local d = gl(n - 1)
  local e = { d }
  return a[1] + b[1] + c[1] + e[1]
end
local s = 0
for i = 1, 40 do s = s + fib(i % 11) + tri(i % 9) end
local r = {}
for i = 1, 6 do r[#r + 1] = fib(10 + i % 3) .. ":" .. tostring(ev(i)) .. ":" .. va(i) end
print(s, table.concat(r, ","), ack(2, 3), fib(14), fib(8.0), fib(7.5))
print(pcall(deep, 1))
print(pcall(zz, 0))
local g = 0
for i = 1, 30 do g = g + gl(i % 12) end
print(g)
print(xpcall(bad, function(m) return (debug.traceback(m, 1):gsub("0x%x+", "PTR")) end, 12))
print(pcall(fib, "x"))
print(select(2, pcall(fib, nil)))
local co = coroutine.wrap(function() local function y(n) if n == 0 then coroutine.yield(1) return 0 end return y(n - 1) + 1 end return y(9) end)
print(co(), co())
local tab = {}
local function mk(n) if n == 0 then return {} end local t = mk(n - 1); t[#t + 1] = n; return t end
for i = 1, 20 do tab[i] = #mk(i * 4) end
collectgarbage()
print(table.concat(tab, ","), fib(16))
"#,
    },
    Prog {
        name: "bm_rec_top",
        small: true,
        src: r#"
-- BUNDLE-M: the callee frame pushed by the native self-call of a function-entry trace has ci->top = func + 1 + maxstacksize, like the frame the
-- interpreter pushes. `__citop()` (a harness native) reads the calling frame's top relative to its base; the same program prints the same
-- numbers whatever pushed the frames.
local function rt(n)
  if n == 0 then return 0 end
  local a, b, c = n, n + 1, n + 2
  local r = rt(n - 1)
  local t = __citop()
  return r + a + b + c + t
end
local s = 0
for i = 1, 4000 do s = s + rt(i % 9) + rt(i % 5) end
print(s)
"#,
    },
    Prog {
        name: "pm3_rec2",
        small: true,
        src: r#"
-- P-M3: the native self-recursion program without the stack-limit cases (those are quadratic under the stack-move stress)
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function tri(n) if n < 3 then return 1 end return tri(n - 1) + tri(n - 2) + tri(n - 3) end
local function bad(n) if n == 0 then error("boom") end return bad(n - 1) + 1 end
local function gl(n)
  if n == 0 then return 0 end
  local a, b, c = { n }, { n + 1 }, { n + 2 }
  local d = gl(n - 1)
  local e = { d }
  return a[1] + b[1] + c[1] + e[1]
end
local s = 0
for i = 1, 60 do s = s + fib(i % 11) + tri(i % 9) + gl(i % 7) end
print(s, fib(12), fib(7.0))
print(xpcall(bad, function(m) return (debug.traceback(m, 1):gsub("0x%x+", "PTR")) end, 9))
print(pcall(fib, "x"))
local co = coroutine.wrap(function() local function y(n) if n == 0 then coroutine.yield(1) return 0 end return y(n - 1) + 1 end return y(7) end)
print(co(), co())
"#,
    },
    Prog {
        name: "be_queens",
        small: false,
        src: r##"
-- BUNDLE-E: awfy `queens` (a method that recurses inside a `for` loop, called through guarded helper methods) plus a variant that
-- fails the native-return guards on purpose (the recursion is entered once through `pcall`, whose callee frame is `CIST_FRESH`, and
-- once from a coroutine). With the `contok` test knob the continuation heads and native returns of the loop recursion are recorded
-- in side traces entered with virtual frames; their guards used V1..V3 although the frames' function slots were still held there.
local Q = {}
Q.__index = Q
function Q:getRowColumn(r, c)
  return self.freeRows[r + 1] and self.freeMaxs[c + r + 1] and self.freeMins[c - r + 8]
end
function Q:setRowColumn(r, c, v)
  self.freeRows[r + 1] = v
  self.freeMaxs[c + r + 1] = v
  self.freeMins[c - r + 8] = v
end
function Q:placeQueen(c)
  for r = 0, 7 do
    if self:getRowColumn(r, c) then
      self.queenRows[r + 1] = c
      self:setRowColumn(r, c, false)
      if c == 7 then return true end
      if self:placeQueen(c + 1) then return true end
      self:setRowColumn(r, c, true)
    end
  end
  return false
end
local function queens(first)
  local q = setmetatable({ freeRows = {}, freeMaxs = {}, freeMins = {}, queenRows = {} }, Q)
  for i = 1, 8 do q.freeRows[i] = true; q.queenRows[i] = -1 end
  for i = 1, 16 do q.freeMaxs[i] = true; q.freeMins[i] = true end
  local ok = q:placeQueen(0)
  local sig = 0
  for i = 1, 8 do sig = sig * 10 + q.queenRows[i] end
  return ok, sig
end
local good, sig = 0, 0
for it = 1, 40 do
  for i = 1, 10 do
    local ok, s = queens(i)
    if ok then good = good + 1 end
    sig = s
  end
end
print("queens", good, sig)
"##,
    },
    Prog {
        name: "r2_rec",
        small: true,
        src: r##"
-- R2: native recursion beyond `GETUPVAL` callees and `RETURN1`: methods (`SELF`) and globals (`GETTABUP`) as callees, `RETURN0`
-- (no result), recursion inside `for` loops (the continuation traces cross the loop back edge), call sites that want 0 / 1 / 2 / all
-- results from a callee that returns fewer, errors, coroutines, GC and hooks inside the recursion.
local P = {}
P.__index = P
function P:swap(i, j) local t = self.v[i + 1]; self.v[i + 1] = self.v[j + 1]; self.v[j + 1] = t end
function P:permute(n)
  self.count = self.count + 1
  if n ~= 0 then
    local n1 = n - 1
    self:permute(n1)
    for i = n1, 0, -1 do
      self:swap(n1, i)
      self:permute(n1)
      self:swap(n1, i)
    end
  end
end
local total = 0
for it = 1, 30 do
  local p = setmetatable({ count = 0, v = { 0, 0, 0, 0, 0, 0 } }, P)
  p:permute(it % 5 + 2)
  total = total + p.count + p.v[1] + p.v[6]
end
print("permute", total)
-- hanoi through a global function and a counter table
moves = { n = 0 }
function hanoi(n, a, b, c)
  if n == 0 then return end
  hanoi(n - 1, a, c, b)
  moves.n = moves.n + 1
  hanoi(n - 1, c, b, a)
end
for i = 1, 25 do hanoi(i % 7 + 1, 1, 2, 3) end
print("hanoi", moves.n)
-- call sites that want a different number of results than the (self-)callee returns
local function f(n)
  if n == 0 then return 1 end
  local a, b = f(n - 1)
  assert(b == nil, "b must be nil")
  return a + 1
end
local function g(n)
  if n == 0 then return end
  local x, y, z = g(n - 1)
  assert(x == nil and y == nil and z == nil, "g returns nothing")
end
local function h(n)
  if n == 0 then return 7 end
  return (h(n - 1))
end
local function k(n)
  if n == 0 then return 1 end
  local t = { k(n - 1) }
  return #t + 1, n
end
local function m(n)
  if n == 0 then return 0 end
  return select("#", m(n - 1)) + n
end
local s = 0
for i = 1, 60 do s = s + f(i % 6 + 2) + (g(i % 5 + 1) == nil and 1 or 0) + h(i % 4 + 1) + k(i % 5 + 1) + m(i % 6) end
print("wanted", s)
-- results that end at `top` (a call as the last argument): the native return must leave `top` right behind the results
local function cnt(...) return select("#", ...) end
local function mm(n) if n == 0 then return 0 end return cnt(mm(n - 1)) + cnt(mm(n - 1), 1, 2) + n end
local function m0(n) if n == 0 then return end return cnt(m0(n - 1)) + n end
local w = 0
for i = 1, 400 do w = w + mm(i % 5 + 1) + (m0(i % 4) or 0) end
print("top", w)
-- errors from the recursion, with tracebacks
local function boom(n, t) if n == 0 then error("bottom " .. #t) end t[#t + 1] = n; local r = boom(n - 1, t); return r end
for i = 1, 8 do
  local ok, e = pcall(boom, i, {})
  print(ok, e)
end
print(select(2, xpcall(boom, function(m) return (debug.traceback(m, 1):gsub("0x%x+", "PTR")) end, 6, {})))
-- a method recursion inside a coroutine that yields at several depths
local T = {}
T.__index = T
function T:walk(n)
  if n > 0 then
    self:walk(n - 1)
    for i = 1, 2 do coroutine.yield(n * 10 + i); self:walk(n - 1 - i) end
  end
end
local co = coroutine.wrap(function() setmetatable({}, T):walk(5) end)
local acc = 0
for i = 1, 100 do local v = co(); if v == nil then break end acc = acc + v end
print("coro", acc)
-- garbage and a changing shape in the recursion
local function build(n) if n == 0 then return {} end local t = build(n - 1); t[#t + 1] = { n, tostring(n) }; return t end
local cnt = 0
for i = 1, 40 do cnt = cnt + #build(i % 9) end
collectgarbage()
print("gc", cnt)
-- mixed argument types and a function replaced mid-run
local function sum(n) if n <= 0 then return 0 end for i = 1, 1 do end return n + sum(n - 1) end
local o = 0
for i = 1, 200 do o = o + sum(i % 10) + sum(i % 3 + 0.5) end
sum = function(n) return n end
for i = 1, 20 do o = o + sum(i) end
print("mixed", o)
-- a count hook firing inside the recursion
local hits = 0
debug.sethook(function() hits = hits + 1 end, "", 97)
for i = 1, 30 do local p = setmetatable({ count = 0, v = { 0, 0, 0, 0, 0, 0 } }, P); p:permute(4) end
debug.sethook()
print("hook", hits > 0)
local W = {}
W.__index = W
function W:walk(n) if n == 0 then return 0 end self:walk(n - 1) for i = 1, 1 do end end
local o = setmetatable({}, W)
local P = W
for i = 1, 400 do o:walk(5 + i % 3) end
-- the caller of a recursion root is a C frame (pcall, coroutine body, xpcall, sort comparator): its `CallInfo` node held a Lua frame
-- of the same function before, whose stale `savedpc` is exactly a continuation site; the native return must not take it for the caller
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local cs = 0
for i = 1, 300 do cs = cs + fib(10 + i % 3) end
for i = 1, 200 do
  local ok, v = pcall(fib, 9 + i % 4)
  cs = cs + v
  local co = coroutine.wrap(function(n) return fib(n) end)
  cs = cs + co(8)
  cs = cs + select(2, xpcall(fib, print, 7))
  cs = cs + (table.unpack({ fib(6) }))
end
for i = 1, 100 do pcall(o.walk, o, 6); pcall(P.walk, o, 4) end
local ts = {}
for i = 1, 50 do ts[#ts + 1] = select("#", pcall(o.walk, o, 3)) end
print("cframes", cs, #ts)
"##,
    },
    Prog {
        name: "r2_rec_deep",
        small: true,
        src: r##"
-- R2: a method recursion with a loop to the stack limit (caught by pcall, with a traceback depth), then normal runs again
local D = {}
D.__index = D
function D:down(n)
  for i = 1, 1 do n = n + 0 end
  self:down(n + 1)
end
function D:ok(n) if n == 0 then return end self:ok(n - 1) for i = 1, 1 do end end
local o = setmetatable({}, D)
for i = 1, 60 do o:ok(i % 9) end
local ok, e = pcall(o.down, o, 1)
print(ok, (tostring(e):gsub("0x%x+", "PTR")):sub(1, 60))
local tb = select(2, xpcall(o.down, debug.traceback, o, 1))
print(#tb > 100, select(2, tb:gsub("\n", "\n")) > 10)
for i = 1, 60 do o:ok(i % 7) end
print("done")
"##,
    },
    Prog {
        name: "r2_side_room",
        small: true,
        src: r##"
-- R2: side traces that start inside inlined frames and need more stack / spare `CallInfo` nodes than their parent trace checked
-- (rare branches that call an intrinsic or a Lua callee with very many registers); the stack is shrunk by `collectgarbage` between
-- short runs of the hot loop, so the room check at the start of such a side trace fails now and then and must leave through the
-- start snapshot (restoring the virtual frames) instead of running the trace.
local abs, floor = math.abs, math.floor
local B = {}
B.__index = B
local function wide(a, b)
  local a1,a2,a3,a4,a5,a6,a7,a8,a9,a10,a11,a12,a13,a14,a15,a16,a17,a18,a19,a20,a21,a22,a23,a24,a25,a26,a27,a28,a29,a30,a31,a32,a33,a34,a35,a36,a37,a38,a39,a40,a41,a42,a43,a44,a45,a46,a47,a48,a49,a50,a51,a52,a53,a54,a55,a56,a57,a58,a59,a60 = a+1,b-2,a+3,b-4,a+5,b-6,a+7,b-8,a+9,b-10,a+11,b-12,a+13,b-14,a+15,b-16,a+17,b-18,a+19,b-20,a+21,b-22,a+23,b-24,a+25,b-26,a+27,b-28,a+29,b-30,a+31,b-32,a+33,b-34,a+35,b-36,a+37,b-38,a+39,b-40,a+41,b-42,a+43,b-44,a+45,b-46,a+47,b-48,a+49,b-50,a+51,b-52,a+53,b-54,a+55,b-56,a+57,b-58,a+59,b-60
  return a1+a2+a3+a4+a5+a6+a7+a8+a9+a10+a11+a12+a13+a14+a15+a16+a17+a18+a19+a20+a21+a22+a23+a24+a25+a26+a27+a28+a29+a30+a31+a32+a33+a34+a35+a36+a37+a38+a39+a40+a41+a42+a43+a44+a45+a46+a47+a48+a49+a50+a51+a52+a53+a54+a55+a56+a57+a58+a59+a60
end
function B:deep(a, b) local u1, u2 = a, b; return wide(u1, u2) + u2 end
function B:step(dx)
  self.x = self.x + dx
  if self.x > 100 then
    self.x = 100
    self.v = 0 - abs(self.v) - 1
    self.n = self.n + (self:deep(self.x, 3) % 7)
  end
  if self.x < 0 then
    self.x = floor(-self.x / 2)
    self.v = abs(self.v) + 1
    self.n = self.n + (self:deep(self.x, 2) % 5)
  end
  return self.n
end
local b = setmetatable({ x = 0, v = 1, n = 0 }, B)
local function run(m, k)
  local acc = 0
  for i = 1, m do
    acc = (acc + b:step((i * 37 + k) % 41 - 17)) % 1000003
  end
  return acc
end
local tot = 0
for k = 1, 300 do
  tot = (tot + run(40, k)) % 1000003
  if k % 3 == 0 then collectgarbage() end
end
print(tot, b.x, b.v, b.n)
-- deep recursion first (the stack grows), then back to shallow runs
local function rec(n, f) if n == 0 then return f() end return rec(n - 1, f) + 1 end
local s = 0
for k = 1, 6 do
  s = s + rec(60 * k, function() return run(60, k) end)
  collectgarbage()
  s = s + run(50, k + 7)
end
print(s)
"##,
    },
    Prog {
        name: "b2_backend",
        small: true,
        src: r##"
-- B2: scaled array addressing (variable index loads and stores of every element type, constants that need two 32-bit halves),
-- the metatable-free array read of a non-nil element (a metatable that appears later, elements that become nil: `__index` must
-- run; `__newindex` of a nil slot must run), the known-tag store skip (type changes, aliased tables).
local N = 600
-- 1. element types through variable indexes
local f, i64, str, bol, tab = {}, {}, {}, {}, {}
for i = 1, N do f[i] = i * 0.5; i64[i] = i; str[i] = "s" .. (i % 10); bol[i] = (i % 3 == 0); tab[i] = { i } end
local sf, si, ss, sb, st = 0.0, 0, 0, 0, 0
for r = 1, 5 do
  for i = 1, N do
    sf = sf + f[i]; si = si + i64[i]; ss = ss + #str[i]; if bol[i] then sb = sb + 1 end; st = st + tab[i][1]
  end
end
print(sf, si, ss, sb, st)
-- stores through variable indexes, including constants that do not fit 32 bits and booleans
local w = {}
for i = 1, N do w[i] = 0 end
for r = 1, 4 do
  for i = 1, N do
    local k = N + 1 - i
    w[k] = 1099511627776 + i      -- (constant folded or not: 64-bit payload)
  end
  for i = 1, N do w[i] = w[i] + 1.5 end
  for i = 1, N do w[i] = (i % 2 == 0) end
  for i = 1, N do w[i] = 123456789012345.0 end
  for i = 1, N do w[i] = i end
end
local sw = 0
for i = 1, N do sw = sw + w[i] end
print(sw)
local rev = {}
for i = 1, N do rev[i] = i end
for r = 1, 3 do for i = N, 1, -1 do rev[i] = rev[N + 1 - i] + r end end
print(rev[1], rev[N], rev[N // 2])
-- 2. metatable appears later: reads of non-nil elements are raw; nil elements go through __index; stores into nil slots through __newindex
local data = {}
for i = 1, 200 do data[i] = i end
local hits, nhits = 0, 0
local mt = { __index = function(t, k) hits = hits + 1; return -k end, __newindex = function(t, k, v) nhits = nhits + 1; rawset(t, k, v) end }
local acc = 0
for r = 1, 40 do
  if r == 10 then setmetatable(data, mt) end
  if r == 20 then data[50] = nil; data[60] = nil end
  if r == 30 then data[50] = 7 end          -- __newindex: the slot is nil
  for i = 1, 200 do acc = acc + data[i] end
end
print(acc, hits, nhits)
-- stores into nil slots of an array that gets `__newindex` later (the store keeps its metatable guard)
local ns = {}
for i = 1, 100 do ns[i] = i end
local nlog = 0
local nmt = { __newindex = function(t, k, v) nlog = nlog + 1; rawset(t, k, v) end }
for r = 1, 40 do
  if r == 15 then setmetatable(ns, nmt) end
  for i = 51, 100 do ns[i] = nil end
  for i = 51, 100 do ns[i] = r end
end
local nsum = 0
for i = 1, 100 do nsum = nsum + ns[i] end
print(nsum, nlog)
-- arrays of arrays where some rows get a metatable (class-like), some elements nil
local rows = {}
for i = 1, 30 do rows[i] = {}; for j = 1, 30 do rows[i][j] = i + j end end
local tot, miss = 0, 0
local rmt = { __index = function(t, k) miss = miss + 1; return 0 end }
for r = 1, 30 do
  if r == 12 then for i = 1, 30, 3 do setmetatable(rows[i], rmt) end end
  if r == 18 then for i = 1, 30, 3 do rows[i][5] = nil end end
  for i = 1, 30 do local row = rows[i]; for j = 1, 30 do tot = tot + row[j] end end
end
print(tot, miss)
-- 3. known tags: field read then written with the same type, type changes, aliasing
local o = { x = 1, y = 2.5, s = "a", b = true }
local sx = 0
for i = 1, 3000 do
  o.x = o.x + 1
  o.y = o.y * 1.0001
  if i == 1000 then o.x = 0.5 end            -- the field changes type
  if i == 2000 then o.x = "7" end
  if i == 2500 then o.x = 3 end
  sx = sx + (type(o.x) == "number" and o.x or 0)
end
print(sx, o.x, o.y)
local a = { x = 1 }
local b = a
local c = { x = 1 }
local sa = 0
for i = 1, 2000 do
  local v = a.x
  b.x = 2.5                                  -- the same node, another type
  a.x = v
  sa = sa + a.x + c.x
  c.x = c.x + 1
  if i % 500 == 0 then b = c end
end
print(sa, a.x, b.x, c.x)
local t1, t2 = { v = 1 }, { v = 1.5 }
local sv = 0
for i = 1, 2000 do
  local p, q = t1, t2
  if i % 2 == 0 then p, q = t2, t1 end
  local x = p.v
  q.v = x
  p.v = x
  sv = sv + (math.type(t1.v) == "integer" and 1 or 2) + (math.type(t2.v) == "integer" and 10 or 20)
end
print(sv, t1.v, t2.v)
"##,
    },
    Prog {
        name: "b4_backend",
        small: true,
        src: r##"
-- B4-1 (`nopay`): stores of nil / false / true write the tag only. The old payload (an integer, a float, a pointer) stays in the slot
-- and must never be seen: equality, rawequal, next, #, unpack, copies, GC (a stale pointer under a false tag), table.move, sort.
local N = 300
local a, h = {}, {}
for i = 1, N do a[i] = i * 7919; h["k" .. (i % 20)] = i end
local out = {}
for r = 1, 6 do
  for i = 1, N do a[i] = (i + r) % 3 == 0 end            -- booleans over integers
  local nt = 0
  for i = 1, N do if a[i] == true then nt = nt + 1 end end
  for i = 1, N do a[i] = i * 31 + r end                  -- integers over booleans
  for i = 1, N, 2 do a[i] = nil end                      -- nil over integers
  local c, s = 0, 0
  for k, v in pairs(a) do c = c + 1; s = s + v end
  local u = { table.unpack(a, 1, 10) }
  for i = 1, N do a[i] = i + 0.25 end                    -- floats
  for i = 1, N do a[i] = false end
  local nf = 0
  for i = 1, N do if a[i] == false then nf = nf + 1 end; if rawequal(a[i], false) then nf = nf + 1 end end
  for i = 1, N do a[i] = { i } end                        -- tables over false
  for i = 1, N do a[i] = (i % 4 == 0) and nil or true end
  collectgarbage()
  local nn = 0
  for i = 1, N do if a[i] == nil then nn = nn + 1 elseif a[i] then nn = nn + 100 end end
  out[#out + 1] = nt .. ":" .. c .. ":" .. s .. ":" .. #u .. ":" .. nf .. ":" .. nn
end
print(table.concat(out, " "))
-- the same through hash fields
local o = { x = 1, y = 2.5, z = "s", w = {} }
local acc = 0
for i = 1, 2000 do
  o.x = (i % 3 == 0)
  o.y = nil
  o.z = true
  acc = acc + (o.x and 1 or 0) + (o.y == nil and 2 or 0) + (o.z and 4 or 0)
  o.x = i; o.y = i * 0.5; o.z = "t" .. (i % 4)
  acc = acc + o.x + o.y
  o.w = (i % 5 == 0) and false or {}
  if o.w == false then acc = acc + 1000 end
end
print(acc, o.x, o.y, o.z, type(o.w))
-- next / # / table.move / sort over arrays that had payloads under nil / false / true
local m = {}
for i = 1, 64 do m[i] = i end
for r = 1, 10 do
  for i = 1, 64 do m[i] = (i % 2 == 0) and false or i end
  local n = 0
  for _, v in ipairs(m) do n = n + 1 end
  local t2 = table.move(m, 1, 64, 1, {})
  local c = 0
  for i = 1, 64 do if t2[i] == false then c = c + 1 end end
  for i = 33, 64 do m[i] = nil end
  print(r, n, c, #m == 32 or #m == 0 or #m)
  for i = 1, 64 do m[i] = i * r end
end
local keys = {}
for i = 1, 100 do keys[i] = i end
for i = 1, 100 do keys[i] = (i % 2 == 0) end
local ok, err = pcall(table.sort, keys)
print(ok, (tostring(err):gsub("^.-:%d+: ", "")))
"##,
    },
    Prog {
        name: "b4_div",
        small: true,
        src: r##"
-- B4-4 (`divmagic`): `//` and `%` by integer constants (magic-number division): every kind of divisor (small, powers of two +- 1,
-- huge, negative, 1, -1, 2^k, the ones that fall back to idiv) against dividends that include the edges of the range and negative
-- values; destination = operand register and not; mixed with float operands; a zero divisor as a constant still raises.
local mi, ma = math.mininteger, math.maxinteger
local ops = { 0, 1, -1, 2, -2, 3, -3, 6, 7, -7, 9, 10, -10, 11, 99, 100, -100, 101, 255, 256, -256, 1000, 1000003, -1000003, 65535, 65536,
  2147483647, -2147483648, 2147483648, 4294967295, 4294967296, 1 << 40, -(1 << 40), (1 << 53) + 1, 1 << 62, -(1 << 62), ma, ma - 1, mi, mi + 1, mi + 2,
  ma // 3 * 3, ma // 7 * 7, -(ma // 7 * 7), 123456789012345, -123456789012345 }
local divs = { 3, 5, 7, 9, 10, 11, 12, 100, 255, 1000, 1000003, 65537, 2147483647, 2147483649, 4294967297, 1099511627777, 3074457345618258603,
  4611686018427387903, 4611686018427387904, 4611686018427387905, ma, 2, 4, 8, 1024, 1, -1, -2, -3, -7, -10, -1000003, mi, mi + 1 }
local src = {}
for i, c in ipairs(divs) do
  local k = (c < 0) and ("(" .. c .. ")") or tostring(c)
  if c == mi then k = "(-9223372036854775807 - 1)" end
  src[#src + 1] = ("local function d%d(x) return x // %s, x %% %s end"):format(i, k, k)
  src[#src + 1] = ("local function e%d(x) local y = x; y = y // %s; x = x %% %s; return y + x end"):format(i, k, k)
end
src[#src + 1] = "return { " .. (function() local t = {} for i = 1, #divs do t[#t + 1] = "d" .. i .. ", e" .. i end return table.concat(t, ", ") end)() .. " }"
local fns = assert(load(table.concat(src, "\n")))()
local acc, cnt = 0, 0
local function mix(v) acc = (acc * 31 + v) % 1000000007; cnt = cnt + 1 end
for round = 1, 40 do
  for fi = 1, #divs do
    local f, g = fns[2 * fi - 1], fns[2 * fi]
    for _, x in ipairs(ops) do
      local q, r = f(x)
      mix(q % 1000003); mix(r % 1000003)
      mix(g(x) % 1000003)
    end
  end
end
print(acc, cnt)
-- the exact values: check against the definition (no division operator: repeated subtraction is too slow, so use the identity with floats
-- where exact, and the invariants q * c + r == x, 0 <= r < c for positive c)
local bad = 0
for fi = 1, #divs do
  local c = divs[fi]
  local f = fns[2 * fi - 1]
  for _, x in ipairs(ops) do
    local q, r = f(x)
    if c > 0 then
      if not (r >= 0 and r < c) then bad = bad + 1 end
    elseif c < 0 and c ~= mi then
      if not (r <= 0 and r > c) then bad = bad + 1 end
    end
    if q * c + r ~= x then bad = bad + 1 end
  end
end
print("bad", bad)
-- the loop forms of the optimised code: operands that change every iteration, negative and positive, float operands in between
local s1, s2, s3 = 0, 0, 0
for i = -3000, 3000 do
  s1 = s1 + i // 7 + i % 7
  s2 = s2 + (i * 2654435761) // 1000003 + (i * 2654435761) % 1000003
  s3 = s3 + (i * 0.5) // 3 + (i * 0.5) % 3
  local j = i
  j = j % 10
  s1 = s1 + j
end
print(s1, s2, s3)
-- a zero divisor in a constant: the error is raised
print(pcall(load("local x = ... return x // 0"), 5))
print(pcall(load("local x = ... return x % 0"), 5))
"##,
    },
    Prog {
        name: "j1_hk",
        small: true,
        src: r##"
-- J1-1: fused HREFK+HLOAD with two exits (a key-tag miss leaves through the HREFK exit, a value-type miss through the HLOAD exit):
-- two objects of one shape with different value types read alternately, a key that moves out of its node, a field that changes type once
-- the trace is hot, and the same reads under a frame-ful call.
local objs = { { a = 1, b = 2, c = 3 }, { a = 1.5, b = 2.5, c = 3.5 }, { a = 1, b = 2, c = 3 } }
local s = 0
for i = 1, 6000 do
  local o = objs[i % 3 + 1]
  s = s + o.a + o.b + o.c
  if i == 3000 then objs[3].a = 0.25 end
  if i == 4000 then objs[1].b = "5" end
  if i == 5000 then objs[1].b = 7 end
end
print(s)
local function get(o) return o.k end
local t = { k = 1, j = 2 }
local acc = 0
for i = 1, 5000 do
  acc = acc + get(t)
  if i == 2500 then t.k = 2.5 end
  if i == 3500 then t.k = nil; t.k = 4 end
  if i == 4500 then for q = 1, 20 do t["x" .. q] = q end end
end
print(acc)
local m = { { v = 1, w = 2 }, { w = 2, v = 1.5 }, { u = 0, w = 3, v = 2 } }
local r = 0
for i = 1, 6000 do
  local o = m[i % 3 + 1]
  r = r + o.v * o.w
end
print(r)
"##,
    },
    Prog {
        name: "b4_hk",
        small: true,
        src: r##"
-- B4-3 (`hkfuse`): field loads with the key tag and the value tag checked together: fields that change type late (integer to float to
-- string to nil to table, true / false), keys that move (rehash), missing keys, objects of the same shape with different value types,
-- a field read right after a write of another type, method tables, and exits in the middle of a loop body.
local function body(bodies, n, r)
  local acc = 0
  for i = 1, n do
    local b = bodies[(i + r) % #bodies + 1]
    local dx = b.x - 1.5
    local dy = b.y - 2.5
    local d2 = dx * dx + dy * dy + b.mass
    acc = acc + d2 * b.vx + b.vy
    b.x = b.x + b.vx * 0.01
    b.y = b.y + b.vy * 0.01
    b.hits = b.hits + 1
  end
  return acc
end
local bodies = {}
for i = 1, 7 do bodies[i] = { x = i + 0.5, y = i * 2.0, vx = 0.25 * i, vy = -0.5 * i, mass = i, hits = 0 } end
local tot = 0
for r = 1, 60 do
  tot = tot + body(bodies, 100, r)
  if r == 20 then bodies[3].mass = 2.5 end               -- an integer field becomes a float
  if r == 30 then bodies[5].hits = 0.5 end               -- ... in another object
  if r == 40 then bodies[2].vx = 3 end                   -- a float field becomes an integer
  if r == 45 then bodies[4].mass = "7" end               -- a number field becomes a string: arithmetic converts
end
print(tot)
bodies[6].vy = nil                                       -- a field becomes nil in a compiled loop: the error is raised
print(pcall(body, bodies, 100, 1))
bodies[6].vy = -1.5
bodies[4].mass = 4
-- key movement: adding keys rehashes and moves the node that held `x`
local o = { x = 1, y = 2, z = 3 }
local s = 0
for i = 1, 3000 do
  local yy = o.y
  if yy == true then yy = 1 end
  s = s + o.x + yy + o.z
  if i % 500 == 0 then o["k" .. i] = i; o.z = o.z + 0.5 end
  if i == 1500 then o.y = true end
  if i == 1501 then o.y = 7 end
  if i == 2500 then o.x = {} ; o.x = 9 end
end
print(s)
-- missing key: nil read through a field that does not exist yet, then exists
local m = { a = 1 }
local c = 0
for i = 1, 2000 do
  if m.b == nil then c = c + 1 else c = c + m.b end
  if i == 700 then m.b = 5 end
  if i == 1400 then m.b = nil end
  if i == 1700 then m.b = 2.5 end
end
print(c)
-- booleans and strings and tables as field values, loaded then used
local objs = {}
for i = 1, 5 do objs[i] = { flag = (i % 2 == 0), name = "n" .. i, sub = { v = i }, cnt = i } end
local k = 0
for r = 1, 80 do
  for i = 1, 5 do
    local ob = objs[i]
    if ob.flag then k = k + ob.cnt end
    k = k + #ob.name + ob.sub.v
  end
  if r == 30 then objs[2].flag = 1 end
  if r == 40 then objs[3].name = "longer name" end
  if r == 50 then objs[4].sub = { v = 100 } end
  if r == 60 then objs[1].cnt = 1.5 end
end
print(k)
-- the same shape with another key order: the slot index differs between objects
local shapes = {}
for i = 1, 6 do
  local t = {}
  if i % 2 == 0 then t.a = 1; t.b = 2; t.c = 3 else t.c = 3; t.b = 2; t.a = 1 end
  if i == 5 then t.a = 1.5 end
  shapes[i] = t
end
local q = 0
for r = 1, 200 do
  for i = 1, 6 do local t = shapes[i]; q = q + t.a + t.b + t.c end
end
print(q)
-- method tables and metatables with fields
local C = { n = 0 }
C.__index = C
function C.inc(self, d) self.n = self.n + d; return self.n end
local inst = setmetatable({}, C)
local z = 0
for i = 1, 3000 do z = z + inst:inc(1) end
print(z, inst.n, C.n)
"##,
    },
    Prog {
        name: "b4_poll",
        small: true,
        src: r##"
-- B4-2 (`pollbud`): loops without helper calls poll once per entry and notice hooks / interrupts when a stay ends; loops with calls keep
-- their per-iteration poll. A hook installed in the middle of a hot loop that calls natives must take effect in the next iteration.
local function sum(n)                                  -- call-free
  local s = 0
  for i = 1, n do s = s + i % 7 end
  return s
end
local acc = 0
for r = 1, 30 do acc = acc + sum(2000 + r) end
print(acc)
local hits = 0
local function hook() hits = hits + 1 end
local t = {}
local tot = 0
for i = 1, 12000 do
  t[i % 8 + 1] = i
  local s = tostring(i)                                -- a native call: the trace keeps its poll
  tot = tot + #s
  if i == 6000 then debug.sethook(hook, "l") end
  if i == 6040 then debug.sethook() end
end
print(tot, hits)
-- a count hook installed before a call-free hot loop: the loop must not run past the hook's budget in compiled code
local calls = 0
local function chook() calls = calls + 1 end
debug.sethook(chook, "", 1000)
local x = 0
for i = 1, 50000 do x = x + i % 3 end
debug.sethook()
print(x, calls > 20)
-- the same loop again after the hook is gone (compiled code is entered again)
local y = 0
for i = 1, 50000 do y = y + i % 3 end
print(y)
"##,
    },
    Prog {
        name: "r2_nilstore",
        small: true,
        src: r##"
-- R2: stores into existing hash nodes that hold nil (a field set to nil and set again), nil stores to absent keys, `__newindex` that
-- appears later or is present from the start, event-name keys (`__index` set after being nil: the metatable cache bits must follow),
-- collectable values into nil slots (write barrier under GC stress).
local Q = {}
Q.__index = Q
local function new() return setmetatable({ head = nil, tail = nil, n = 0, name = "q" }, Q) end
function Q:push(v)
  local node = { v = v, nxt = nil }
  if self.tail then self.tail.nxt = node else self.head = node end
  self.tail = node
  self.n = self.n + 1
end
function Q:pop()
  local h = self.head
  if h == nil then return nil end
  self.head = h.nxt
  if self.head == nil then self.tail = nil end
  h.nxt = nil
  self.n = self.n - 1
  return h.v
end
local q = new()
local s = 0
for i = 1, 3000 do
  q:push(i)
  if i % 3 == 0 then s = s + q:pop() + (q:pop() or 0) end
  if i % 7 == 0 then q.scratch = nil; q.scratch = i; q.scratch = nil end
end
while q.head do s = s + q:pop() end
print("queue", s, q.n)
-- the same field nil / non-nil with a metatable that gets `__newindex` in the middle of the run
local log = {}
local P = {}
local obj = setmetatable({ slot = nil, other = 1 }, P)
obj.slot = 1
obj.slot = nil
local total = 0
for i = 1, 400 do
  obj.slot = i
  total = total + obj.slot
  obj.slot = nil
  if i == 250 then
    P.__newindex = function(t, k, v) log[#log + 1] = k .. "=" .. tostring(v); rawset(t, k, v) end
  end
  if i == 300 then P.__newindex = nil end
end
print("newindex", total, #log, obj.slot)
-- event-name keys: set to nil and set again, through a table used as a metatable by another. A lookup through the metatable caches the
-- absence of the event; the store into the nil slot must invalidate that cache (like `luaV_finishset`), or a later lookup that was
-- recorded while the event was absent would not notice the function.
local M = { __index = function(t, k) return k .. "!" end }
local u = setmetatable({}, M)
local mi = function(t, k) return k .. "?" end
M.__index = nil
local ev = 0
for i = 1, 3000 do
  local a = u.zz
  ev = ev + (a == nil and 1 or #a)
  if (i * 7) % 5 < 2 then M.__index = mi else M.__index = nil end
end
print("events", ev)
-- nil stores to absent keys, with a metatable that has `__newindex`, then without
local R = setmetatable({}, { __newindex = function(t, k, v) rawset(t, k, v == nil and "nil!" or v) end })
local W = {}
for i = 1, 200 do
  local t = (i % 2 == 0) and R or W
  t.fresh = nil
  if i % 2 == 0 then rawset(t, "fresh", nil) end
end
print("absent", rawget(R, "fresh"), rawget(W, "fresh"))
-- collectable values into slots that held nil (barrier), with a full collection now and then
local holder = { a = nil, b = nil, c = nil }
local keep = 0
for i = 1, 2000 do
  holder.a = { i }
  holder.b = "s" .. i
  holder.c = function() return i end
  keep = keep + holder.a[1] + #holder.b + holder.c()
  if i % 200 == 0 then collectgarbage() end
  holder.a = nil; holder.b = nil; holder.c = nil
end
print("barrier", keep)
-- a key that becomes dead (the slot's value is nil and the table is rehashed by insertions)
local D = { x = nil, y = 1 }
D.x = 1; D.x = nil
local c = 0
for i = 1, 300 do
  D.x = i
  if i % 40 == 0 then for j = 1, 20 do D["k" .. j] = j end; for j = 1, 20 do D["k" .. j] = nil end; collectgarbage() end
  c = c + D.x
  D.x = nil
end
print("dead", c)
"##,
    },
    Prog {
        name: "fnroot_mutual",
        small: true,
        src: r#"
-- P-E2: mutual recursion, and a recursion whose depth changes between calls
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
local function depth(n) if n == 0 then return 0 end return 1 + depth(n - 1) end
local function tree(d) if d == 0 then return 1 end return tree(d - 1) + tree(d - 1) end
local out = {}
for i = 0, 25 do out[#out + 1] = (even(i) and "e" or "o") end
local s = 0
for _, d in ipairs({1, 5, 2, 9, 3, 0, 12, 4, 4, 4, 7, 1, 20, 2}) do s = s + depth(d) + tree(d % 6) end
print(table.concat(out), s, depth(60), tree(7))
"#,
    },
    Prog {
        name: "fnroot_error",
        small: true,
        src: r#"
-- P-E2: errors raised inside recursion (caught by pcall at different depths), error values that are tables, and a call that
-- passes arguments of a type the recorded path did not see
local function boom(n) if n == 0 then error("bottom") end return boom(n - 1) + 1 end
local function chk(n) if n == 0 then error({code = 7}) end return chk(n - 1) + 1 end
local function add(a, b) return a + b end
local function twice(f, x) return f(f(x)) end
local function inc(x) return add(x, 1) end
local res = {}
for i = 1, 40 do
  local ok, e = pcall(boom, i % 5)
  res[#res + 1] = ok and "y" or (type(e) == "string" and #e or 0)
  local ok2, e2 = pcall(chk, i % 4)
  res[#res + 1] = ok2 and "y" or e2.code
  res[#res + 1] = twice(inc, i) + (i % 7 == 0 and twice(inc, i + 0.5) or 0)
  local ok3, e3 = pcall(twice, inc, i % 9 == 0 and "x" or i)
  res[#res + 1] = ok3 and e3 or "err"
end
print(table.concat(res, ","))
"#,
    },
    Prog {
        name: "fnroot_coro",
        small: true,
        src: r#"
-- P-E2: coroutines inside recursion: yields from a recursive generator, resumes in the middle of a recursion, a recursion
-- that runs inside a coroutine body and one that wraps a coroutine
local function walk(n) if n > 0 then walk(n - 1) coroutine.yield(n) walk(n - 1) end end
local function gen(n) return coroutine.wrap(function() walk(n) end) end
local function rec(n) if n == 0 then return coroutine.yield(0) end return 1 + rec(n - 1) end
local s = 0
for n = 1, 6 do for v in gen(n) do s = s + v end end
local co = coroutine.create(function(a) return rec(a) end)
local ok, v = coroutine.resume(co, 5)
local ok2, v2 = coroutine.resume(co, 10)
local function ycount(n) if n == 0 then return 0 end local x = coroutine.yield(n) return x + ycount(n - 1) end
local c3 = coroutine.wrap(function() return ycount(6) end)
local acc = {}
local r = c3()
for i = 1, 6 do acc[#acc + 1] = r r = c3(i) end
print(s, ok, v, ok2, v2, table.concat(acc, ","), r)
"#,
    },
    Prog {
        name: "fnroot_hook",
        small: true,
        src: r#"
-- P-E2: hooks installed while function roots are hot and recorded: the hook sees the same events
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function leaf(x) return x * 2 end
local function mid(x) return leaf(x) + leaf(x + 1) end
local calls = 0
local s = 0
for i = 1, 30 do s = s + fib(i % 7) + mid(i) end
debug.sethook(function() calls = calls + 1 end, "c")
for i = 1, 10 do s = s + fib(i % 5) + mid(i) end
debug.sethook()
local lines = 0
debug.sethook(function() lines = lines + 1 end, "l")
s = s + fib(6)
debug.sethook()
for i = 1, 30 do s = s + fib(i % 7) + mid(i) end
print(s, calls > 0, lines > 0)
"#,
    },
    Prog {
        name: "fnroot_types",
        small: true,
        src: r#"
-- P-E2: the argument types change under a hot function root (entry checks fail, side traces from type-guard exits)
local function f(a, b) if a < b then return a + b end return a - b end
local function g(x) return f(x, x + 1) + f(x + 1, x) end
local function h(t, i) return t[i] + g(i) end
local s = 0
local ta = {}
for i = 1, 40 do ta[i] = i * 1.5 end
for i = 1, 40 do s = s + g(i) end
for i = 1, 40 do s = s + g(i + 0.25) end
for i = 1, 40 do s = s + h(ta, i) end
for i = 1, 20 do s = s + g(math.maxinteger - i) end
for i = 1, 20 do local ok, e = pcall(g, "s") s = s + (ok and 1 or 2) end
print(s)
"#,
    },
    Prog {
        name: "fnroot_dead",
        small: true,
        src: r#"
-- P-E2: a function root recorded on one path that no later call takes (the calls of the first phase decide the recorded path):
-- its entries leave at once and the ineffective-trace rule has to flush it
-- (N1: the `CLOSURE` in the loops keeps the calling loops untraceable, as the `UpvalAlias` abort of `leaf` did before)
local function leaf(x) return x + 1 end
local function f(x)
  if x < 1000 then return leaf(x) + leaf(x - 1) + leaf(x - 2) + leaf(x - 3) end
  return leaf(x) + leaf(x * 2) + leaf(x * 3) + leaf(x * 4)
end
local s = 0
for i = 1, 300 do local _ = function() end s = s + f(i) end
for i = 1, 6000 do local _ = function() end s = s + f(i + 1000) end
print(s)
"#,
    },
    Prog {
        name: "fnroot_mixed",
        small: true,
        src: r#"
-- P-E2: a function root whose recorded path is taken by half of the calls only
local function leaf(x) return x + 1 end
local function f(x)
  if x % 2 == 0 then return leaf(x) + leaf(x * 2) + leaf(x * 3) + leaf(x * 4) end
  return leaf(x) + leaf(x - 2) + leaf(x - 3) + leaf(x - 4)
end
local s = 0
for i = 0, 8000 do local _ = function() end s = s + f(i) end
print(s)
"#,
    },
    Prog {
        name: "fnroot_leafy",
        small: true,
        src: r#"
-- P-E2: a hot function that calls a native and returns at once, called from C (`gsub`): every function-entry recording of it
-- ends short
local function leafy(x) local t = type(x) return t end
local _, n = string.gsub(string.rep("x", 20000), ".", leafy)
print(n)
"#,
    },
    Prog {
        name: "lib_str",
        small: true,
        src: r#"
-- P-B: string.sub / byte / char, the method forms, type / math.type / tostring of numbers, edge indices, short and long results,
-- a library function replaced late, the string metatable changed late, a number metatable
local S = { "hello world", ("abcdefghij"):rep(7), "\0\1\2", "a longer string than forty bytes: 0123456789 abc" }
local function f(n, late, mut)
  local s, acc = 0, ""
  for i = 1, n do
    local str = S[i % #S + 1]
    local a = str:sub(i % 5 - 1, i % 7 - 2)
    local b = string.sub(str, -3)
    local c = str:byte(i % 3 + 1)
    local d = string.char(i % 256)
    local e = #(tostring(i) .. tostring(i / 8)) + #(type(i) .. type(str)) + #(type(nil) .. math.type(i)) + #math.type(i / 2)
    s = s + #a + #b + (c or -1) + #d + e + #str:sub(2)
    if type(i) == "number" and math.type(i) == "integer" then s = s + 1 end
    if i % 5 == 0 then acc = acc .. a:sub(1, 3) .. string.char(65 + i % 26) end
    if i == late then mut() end
  end
  return s, #acc
end
local keep = { string.sub, string.byte, string.char, tostring, type }
local function restore() string.sub, string.byte, string.char, tostring, type = keep[1], keep[2], keep[3], keep[4], keep[5]; debug.setmetatable(0, nil); getmetatable("").__index = string end
local muts = {
  function() end,
  function() string.sub = function() return "S" end end,
  function() string.byte = function() return 7 end end,
  function() tostring = function() return "T" end end,
  function() type = function() return "ty" end end,
  function() debug.setmetatable(0, { __tostring = function() return "N" end }) end,
  function() getmetatable("").__index = { sub = function() return "M" end, byte = string.byte } end,
}
for k, m in ipairs(muts) do
  print(k, pcall(f, 60, 30, m))
  restore()
end
print(pcall(f, 60, 1000, muts[1]))
print(("x"):rep(3):sub(2, 2), pcall(string.char, 256), pcall(string.sub))
for _, e in ipairs{ "", "x", "ab" } do print(e:byte(1), e:byte(-1), e:byte(5), e:sub(0), e:sub(2, 1), tostring(math.type(e))) end
"#,
    },
    Prog {
        name: "lib_tab",
        small: true,
        src: r#"
-- P-B: table.insert / table.remove (append and pop), rawlen, math.fmod / tointeger / tonumber of numbers, a metatable added late,
-- a sparse table, an empty table
local function f(t, n, late, mut)
  local s = 0
  for i = 1, n do
    table.insert(t, i)
    if i % 4 == 0 then s = s + (table.remove(t) or 0) end
    s = s + rawlen(t) + #t
    local x = i + 0.5
    s = s + math.fmod(x, 3.25) + (math.tointeger(i + 0.0) or 0) + tonumber(i) + tonumber(x) + (math.tointeger(x) or -1)
    if i == late then mut(t) end
  end
  return s, #t, t[1], t[#t]
end
local muts = {
  function() end,
  function(t) setmetatable(t, { __newindex = function(t, k, v) rawset(t, k, v * 2) end }) end,
  function(t) setmetatable(t, { __len = function() return 0 end }) end,
  function(t) t[#t + 5] = 1 end,
  function(t) for k = 1, #t do t[k] = nil end end,
}
for k, m in ipairs(muts) do
  print(k, pcall(f, {}, 50, 20, m))
  print(k, pcall(f, { 1, 2, 3 }, 50, 20, m))
end
print(pcall(f, setmetatable({}, { __len = function() return 2 end }), 7, 1000, muts[1]))
print(pcall(f, { n = 1, [100] = 5 }, 50, 1000, muts[1]))
local e = {}
for k = 1, 5 do print(table.remove(e)) end
print(pcall(table.insert, e, 1, 2, 3), pcall(table.insert, nil, 1), pcall(math.fmod, 1, 0))
"#,
    },
    Prog {
        name: "pl_varargs_exit",
        small: true,
        src: r#"
-- P-L: a loop whose trace exits at the same snapshot every iteration (a vararg call it does not inline): the exit stub
-- keeps its counter in memory when the snapshot reads every scratch register, and the hot divert must still fire
local function sum(...) local s = 0; for i = 1, select('#', ...) do s = s + (select(i, ...)) end; return s end
local function pass(...) return ... end
local function pack2(...) local t = table.pack(...); return t.n end
local t = {1, 2, 3, 4, 5, 6, 7, 8}
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + sum(i, 2, 3, 4) + select('#', pass(i, i, i)) + pack2(i, i) + select(2, table.unpack(t, 1, 5))
  end
  return s
end
print(run(300), run(2000), run(0))
"#,
    },
    Prog {
        name: "pn_pow2_divmod",
        small: true,
        src: r#"
-- P-N: `%` and `//` by constant powers of two are an AND / an arithmetic shift in the trace (no idiv, no sign fix-up):
-- negative and mixed-sign dividends, minint, 2^31 (the largest immediate mask), 2^32 (the generic path), the floor rule
local function run(n)
  local s, t, u, w = 0, 0, 0, 0
  local mi, ma = math.mininteger, math.maxinteger
  for i = 1, n do
    local a = (i * 7919) % 2001 - 1000
    s = s + a % 4 + a % 2 * 3 + a % 1024
    t = t + a // 4 + a // 2 + a // 1024
    local b = mi + i
    local c = ma - i
    u = u + b % 8 + b // 8 + c % 8 + c // 8 + (b % 2147483648) + (b // 2147483648) + (c % 4294967296) // 3
    w = w + (-a) % 16 + (-a) // 16 + (a - 5) % 2 + (a - 5) // 2
  end
  return s, t, u, w
end
print(run(400))
print(run(3000))
print(run(0))
"#,
    },
    Prog {
        name: "pv_string_methods",
        small: true,
        src: r#"
-- P-V: `s:method(...)` on a string receiver resolves through the string metatable as a guarded constant in a trace. The metatable
-- (`debug.setmetatable`), its `__index` field, the table behind it, a method of that table, an `__index` chain or function are
-- changed in the middle of a hot loop (at iteration k, for several k: before the trace exists, while it runs).
local mt0 = getmetatable("")
local idx0 = mt0.__index
local osub, obyte, oupper = string.sub, string.byte, string.upper
string.zz = function(s) return #s + 1 end
local function hot(s, n, k, mut)
  local acc = 0
  for i = 1, n do
    if i == k then mut() end
    local c = s:sub(i % 5 + 1, i % 5 + 1)
    acc = acc + #c + s:byte(i % 7 + 1) + s:zz()
  end
  return acc
end
local function try(name, mut, restore)
  for _, k in ipairs{ 3, 40, 150, 700, 1500 } do
    local ok, r = pcall(hot, "hello world", 2000, k, mut)
    restore()
    print(name, k, ok, r)
  end
end
local function restore_all()
  string.sub, string.byte, string.upper = osub, obyte, oupper
  string.zz = function(s) return #s + 1 end
  if mt0.__index ~= idx0 then mt0.__index = idx0 end
  if getmetatable("") ~= mt0 then debug.setmetatable("", mt0) end
end
try("none", function() end, restore_all)
try("sub_lua", function() string.sub = function(s, i, j) return osub(s, i, j) .. "" end end, restore_all)
try("sub_other", function() string.sub = function(s, i, j) return "ab" end end, restore_all)
try("sub_c", function() string.sub = string.upper end, restore_all)
try("sub_nil", function() string.sub = nil end, restore_all)
try("zz_changed", function() string.zz = function(s) return 7 end end, restore_all)
try("zz_nil", function() string.zz = nil end, restore_all)
try("idx_table", function() mt0.__index = { sub = osub, byte = obyte, zz = function() return 2 end } end, restore_all)
try("idx_chain", function() mt0.__index = setmetatable({}, { __index = string }) end, restore_all)
try("idx_chain_sub", function()
  mt0.__index = setmetatable({ sub = function(s, i, j) return osub(s, i, j) end }, { __index = string })
end, restore_all)
try("idx_func", function() mt0.__index = function(s, key) return string[key] end end, restore_all)
try("idx_nil", function() mt0.__index = nil end, restore_all)
try("idx_str", function() mt0.__index = "x" end, restore_all)
try("mt_new", function() debug.setmetatable("", { __index = { sub = osub, byte = obyte, zz = function() return 5 end } }) end, restore_all)
try("mt_nil", function() debug.setmetatable("", nil) end, restore_all)
try("mt_same_new_idx", function() debug.setmetatable("", { __index = string }) end, restore_all)
-- the method added after the trace exists (absent key -> present), and a rehash of the string table
try("grow", function() for i = 1, 40 do string["extra" .. i] = i end end, function() for i = 1, 40 do string["extra" .. i] = nil end restore_all() end)
-- long-string receiver
local long = ("x"):rep(100)
local function hot2(n, k, mut)
  local acc = 0
  for i = 1, n do
    if i == k then mut() end
    acc = acc + #long:sub(i % 50 + 1, i % 50 + 60) + long:byte(i % 90 + 1)
  end
  return acc
end
for _, k in ipairs{ 5, 100, 1000 } do
  print("long", k, pcall(hot2, 1500, k, function() string.byte = function(s, i) return 1 end end))
  restore_all()
end
-- error message of a missing method
print(pcall(function() local s = "a"; for i = 1, 100 do s:nomethod() end end))
print(pcall(function() local s = "a"; for i = 1, 300 do if i == 250 then string.sub = nil end; local c = s:sub(1, 1) end end))
restore_all()
"#,
    },
    Prog {
        name: "px_typed_landing",
        small: true,
        src: r#"
-- P-X: an exit of a trace lands on the tier-1 label of its pc. Tier 1 types the registers inside a block (MODK / IDIVK /
-- SHRI.. leave an Int when the float case exits to the interpreter), the trace handles the float case itself, so the
-- label must verify the tags it assumes. `a` turns into a float in one iteration; the side trace recorded at that
-- iteration (float `%`, `//`, `<<` results) is entered by later calls whose float fails its guard right behind the
-- operation: the exit lands on the compare / add that follows the typed instruction.
local function fmod(n, off)
  local s = 0
  for i = 1, n do
    local a = (i * 786) % 301 - 150 + off
    if i == 24 then a = a + 0.5 end
    s = s + (a % 4 > 1 and 1 or 0)
  end
  return s
end
local function fidiv(n, off)
  local s = 0
  for i = 1, n do
    local a = (i * 786) % 301 - 150 + off
    if i == 24 then a = a + 0.5 end
    s = s + (a // 4 > 12 and 1 or 0)
  end
  return s
end
local function fmodm(n, off)
  local s, m = 0, 0
  for i = 1, n do
    local a = (i * 786) % 301 - 150 + off
    if i == 24 then a = a + 0.5 end
    local r = (a % 4 > 1 and 1 or 0)
    s = s + r
    m = m | (r << i)
  end
  return s, m
end
for _, k in ipairs{1, -3, 4, -8, 5, 2, -1, -6} do
  print(k, fmod(40, k), fidiv(40, k), fmodm(40, k))
end
"#,
    },
    Prog {
        name: "po_protected_calls",
        small: true,
        src: r#"
-- P-O: `pcall` / `xpcall` of a Lua closure inside a hot loop is part of the trace; an error of the body leaves the trace before the
-- call (the interpreter then runs the real `pcall`). Result counts, error values, a handler, a non-function, recursion through
-- `pcall` up to the C-stack limit, and a hook set in the middle.
local function ok1(i) return i + 1 end
local function ok3(i, j) return i, j, i + j end
local function ok0() end
local function flt(x) return x * 0.5 end
local function maybe(i, k) if i == k then error("boom" .. i) end return i end
local function idx(t, i) return t[i].v end
local function arith(a, b) return a + b end
local function cmp(a, b) if a < b then return 1 end return 2 end
local function run(n, k)
  local s = 0
  for i = 1, n do
    local a, b = pcall(ok1, i); s = s + b
    local c, d, e, f = pcall(ok3, i, 2); s = s + e + f
    local g = pcall(ok0); local h, m = pcall(ok0); if g and h and m == nil then s = s + 1 end
    s = s + select('#', pcall(ok3, 1, 2))
    local _, x = pcall(flt, i); s = s + x
    local ok, v = pcall(maybe, i, k)
    if ok then s = s + v else s = s + #v end
    local ok2, v2 = xpcall(maybe, function(m) return "h:" .. m end, i, k)
    if ok2 then s = s + v2 else s = s + #v2 end
    local ok4, v4 = xpcall(ok3, print, i, 3); s = s + v4
  end
  return s
end
print(run(3000, 100000))
print(run(3000, 2999))
print(run(3000, 1))
local T = {{v = 1}, {v = 2}, {v = 3}}
local function run2(n, bad)
  local s = 0
  for i = 1, n do
    local ok, v = pcall(idx, T, i % 3 + 1 + (i == bad and 5 or 0))
    s = s + (ok and v or -1)
    ok, v = pcall(arith, i, i == bad and "x" or 2); s = s + (ok and v or -2)
    ok, v = pcall(arith, i, i == bad and {} or 2); s = s + (ok and v or -3)
    ok, v = pcall(arith, i, i == bad and nil or 2.5); s = s + (ok and v or -4)
    ok, v = pcall(cmp, i, i == bad and {} or 7); s = s + (ok and v or -5)
  end
  return s
end
print(run2(3000, 100000)); print(run2(3000, 2000)); print(run2(3000, 5))
-- non-functions, callables, C functions, vararg callees, recursion
local callable = setmetatable({}, {__call = function(_, x) return x * 2 end})
local function vf(...) return select('#', ...), ... end
local function run3(n)
  local s = 0
  for i = 1, n do
    local ok, v = pcall(callable, i); s = s + (v or 0)
    ok, v = pcall(vf, i, i); s = s + v
    ok, v = pcall(math.abs, -i); s = s + v
    ok, v = pcall(i % 10 == 0 and nil or ok1, i); s = s + (ok and v or 0)
  end
  return s
end
print(run3(2000))
print(pcall(pcall)); print(pcall(xpcall)); print(pcall(xpcall, ok1)); print(pcall(xpcall, ok1, 5))
-- error values of every type, nested pcall inside the body, error in the handler
local function thrower(i)
  local m = i % 6
  if m == 0 then error(i) elseif m == 1 then error({i}) elseif m == 2 then error(nil)
  elseif m == 3 then error(true) elseif m == 4 then error("s" .. i, 0) end
  return i
end
local function nested(i) local ok, v = pcall(thrower, i); return ok, v end
local acc = 0
for i = 1, 3000 do
  local ok, v = pcall(thrower, i); acc = acc + (ok and v or (type(v) == "number" and v or 1))
  local o2, a2, b2 = pcall(nested, i); acc = acc + (o2 and 1 or 0) + (a2 and 1 or 0)
  local o3, v3 = xpcall(thrower, function(e) error("in handler") end, i); acc = acc + (o3 and 1 or 2)
end
print(acc)
-- a to-be-closed variable in the body
local closed = 0
local function tbc(i)
  local x <close> = setmetatable({}, {__close = function() closed = closed + 1 end})
  if i % 7 == 0 then error("tbc") end
  return i
end
for i = 1, 3000 do pcall(tbc, i) end
print(closed)
-- recursion through pcall to the C-stack limit, inside a hot loop
local depth = 0
local function rec(n) depth = depth + 1; if n == 0 then return 0 end local ok, v = pcall(rec, n - 1); if not ok then error(v, 0) end return v + 1 end
for i = 1, 300 do depth = 0; local ok, v = pcall(rec, i % 3 == 0 and 250 or 20); acc = acc + (ok and v or #tostring(v)) end
print(acc, depth)
-- a hook set in the middle of the loop, and the body changing the environment
local cnt = 0
local function body(i) if i == 1500 then debug.sethook(function() cnt = cnt + 1 end, "", 100) end return i end
for i = 1, 3000 do pcall(body, i) end
debug.sethook()
print(cnt > 0)
local gf = function(i) return i end
local function ch(i) return gf(i) end
local s5 = 0
for i = 1, 3000 do
  if i == 1000 then gf = function(i) error("changed") end end
  if i == 2000 then gf = function(i) return -i end end
  local ok, v = pcall(ch, i); s5 = s5 + (ok and v or 7)
end
print(s5)
-- a body with a side effect before a guard is not part of a trace (an exit would repeat the effect)
local c1, tt, tn = 0, {}, 0
local function side(i, k) c1 = c1 + 1; tt[i % 5 + 1] = i; if i == k then error("x") end return i end
local function side2(i, k) local t = {i}; c1 = c1 + #t; if i % k == 0 then error("y") end return i end
for i = 1, 3000 do pcall(side, i, 2500); pcall(side2, i, 1000) end
print(c1, tt[1], tt[2])
-- the C stack: a hot pcall loop first runs shallow (a trace), then at depths near LUAI_MAXCCALLS (the real call raises)
local function loopy(n) local s = 0; for i = 1, n do local ok, v = pcall(ok1, i); if ok then s = s + v else return "E" .. tostring(v) end end return s end
print(loopy(3000))
local function deepcall(d, n) if d == 0 then return loopy(n) end local ok, v = pcall(deepcall, d - 1, n); if not ok then return "D" .. tostring(v) end return v end
print(20, deepcall(20, 3000))
for d = 120, 200 do local r = deepcall(d, 3000); if d % 20 == 0 or r ~= 4504500 then print(d, r) end end
"#,
    },
    Prog {
        name: "px_rec_landing",
        small: true,
        src: r#"
-- P-M4: the P-X landing rule on the native-recursion paths (`recnat=1`). `ra`: an ARGUMENT turns into a float at one depth, the
-- partial-leaf templates (`%`, `//`) behind it are typed Int in tier 1 and the exits of the down trace / its side traces
-- land mid-block. `rb`: a RESULT is a float on the way back (the continuation trace after the native return exits at the
-- compare behind `//` / `%` of the call's result). `rc`: bitwise operators on a result that can be a float with an integral value.
local function ra(n, a)
  if n == 0 then return a end
  if n == 12 then a = a + 0.5 end
  local c = (a % 4 > 1) and 1 or 0
  local d = (a // 2 > 6) and 2 or 0
  return ra(n - 1, a + c + d) + 1
end
local function rb(n, a)
  if n == 0 then return a end
  local r = rb(n - 1, a)
  local c = (r // 2 > 3) and 2 or 0
  local d = (r % 4 > 1) and 1 or 0
  return r + c + d
end
local function rc(n, a)
  if n == 0 then return a end
  local r = rc(n - 1, a)
  local m = (r | 1) > 5 and 3 or 1
  return r + m
end
local function rd(n, a)
  if n == 0 then return a end
  local c = (a % 4 > 1) and 1 or 0
  local d = (a // 2 > 6) and 2 or 0
  return rd(n - 1, a + c + d) + 1
end
local function re(n, a)
  if n == 0 then return a end
  local c = (a % 4 > 1) and 1 or 0
  local r = re(n - 1, a + c)
  local d = (r // 2 > 6) and 2 or 0
  return r + d
end
local s4, s5 = 0, 0
for k = 1, 400 do
  local x = (k * 37) % 23 - 11
  if k % 11 == 5 or k % 11 == 8 then x = x + 0.5 end
  s4 = s4 + rd(6 + k % 4, x)
  s5 = s5 + re(3 + k % 4, x)
end
print(s4, s5)
local s1, s2, s3 = 0, 0, 0
for k = 1, 400 do
  local x = (k * 37) % 23 - 11
  local y = x
  if k % 7 == 3 then y = y + 0.5 end
  local z = x
  if k % 5 == 2 then z = z + 0.0 end
  s1 = s1 + ra(16 + k % 3, x)
  s2 = s2 + rb(1 + k % 6, y)
  s3 = s3 + rc(1 + k % 5, z)
end
print(s1, s2, s3, ra(14, 3), rb(3, 2.5), rb(3, 7.5), rb(3, -7.5), rc(3, 4.0), rc(3, 9))
"#,
    },
    Prog {
        name: "pm4_rec_counters",
        small: true,
        src: r#"
-- P-M4: recursion whose argument / result types change between calls (entry checks of the continuation traces fail at an
-- inner entry); the entry counters of the heads must stay sane (`entered` of the statistics line is the sum of `entry_ok`).
local function fb(n) if n < 2 then return n end return fb(n - 1) + fb(n - 2) end
local function fa(n) if n < 3 then return 1 end return fa(n - 1) + fa(n - 2) + fa(n - 3) end
local s = 0
for k = 1, 81 do s = s + fb(7 + k % 4) + fa(9) + fb(5 + k % 3) end
print(s, fb(12))
print(fb(6.0), fb(7), fb(5.5), pcall(fb, "x"))
print(fb(8), fa(7.0), fa(6), fb(9.0))
for k = 1, 60 do s = s + fb(6 + k % 2 * 0.5) + fa(5 + k % 3) end
print(s, fb(10))
"#,
    },
    Prog {
        name: "n5_str_variants",
        small: true,
        src: r##"
-- N5: strings of both variants (short <= 40 bytes, long > 40) read from tables in loops; the variant is observed (#s, ==, key,
-- concat, escape through an exit) or not (unused, carried only).
local S = {}
for k = 1, 8 do S[k] = ("x"):rep(34 + k) end
local O = {{name = ("a"):rep(40), n = 1}, {name = ("b"):rep(41), n = 2}, {name = "c", n = 3}, {name = ("d"):rep(90), n = 4}}
local function unused(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + i % 3 end return acc end
local function field(n) local acc = 0 for i = 1, n do local s = O[i % 4 + 1].name acc = acc + i % 5 end return acc end
local function carry(n) local last = S[1] for i = 1, n do local s = S[i % 8 + 1] if i % 5 == 0 then last = s end end return last end
local function out(n) local s for i = 1, n do s = S[i % 8 + 1] end return s end
local function early(n, k) for i = 1, n do local s = S[i % 8 + 1] if i == k then return s, i end end return nil end
local function len(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + #s end return acc end
local function eq(n) local t, c = S[3], 0 for i = 1, n do local s = S[i % 8 + 1] if s == t then c = c + 1 end end return c end
local function eql(n, off, t) local c = 0 for i = 1, n do local s = S[(i + off) % 8 + 1] if s == t then c = c + 1 end end return c end
local function eqf(n) local c = 0 for i = 1, n do local a, b = O[i % 4 + 1].name, O[(i + 1) % 4 + 1].name if a == b then c = c + 1 end end return c end
local function key(n) local m, c = {}, 0 for i = 1, n do local s = S[i % 8 + 1] m[s] = (m[s] or 0) + 1 end for _, v in pairs(m) do c = c + v end return c end
local function cat(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + #(s .. "!") end return acc end
local function store(n) local T = {} for i = 1, n do local s = S[i % 8 + 1] T[i % 3 + 1] = s end return #T[1], #T[2], #T[3] end
local function upv(n) local u = "" local function g(s) u = s end for i = 1, n do g(S[i % 8 + 1]) end return #u end
local function sel(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + select("#", s, i) + #type(s) end return acc end
for round = 1, 6 do
  local n = 40 + round * 13
  print(unused(n), field(n), #carry(n), #out(n), len(n), eq(n), eqf(n), key(n), cat(n), store(n))
  print(eql(n, 6, S[1]), eql(n, 6, S[7]), eql(n, 0, S[8]), eql(n, 1, S[2]), eql(n, 7, S[7]))
  print(early(n, 7 + round), upv(n), sel(n), out(n) == S[n % 8 + 1], carry(n) == S[(n - n % 5) % 8 + 1])
end
local x = out(47)
print(#x, x == S[8], type(x), x:sub(1, 3), (x .. x):len(), early(30, 7) == S[7])
"##,
    },
    Prog {
        name: "n5_callee_variants",
        small: true,
        src: r#"
-- N5: the callee of a call (and numbers of both subtypes) read from a table: Lua closures, light C functions and a C closure
-- mixed in one slot.
local function l1(x) return x + 1 end
local function l2(x) return x * 2 end
local gm = ("a b c"):gmatch("%a")
local F = {l1, math.abs, l2, math.floor, tostring, type}
local N = {1, 2.5, 3, 4.25, 5, -6.5, 7, 8}
local function calls(n) local acc = 0 for i = 1, n do local f = F[i % 4 + 1] acc = acc + f(i) end return acc end
local function calls2(n) local acc = 0 for i = 1, n do local f = F[i % 6 + 1] local r = f(i) acc = acc + #tostring(r) end return acc end
local function nums(n) local acc = 0 for i = 1, n do acc = acc + N[i % 8 + 1] end return acc end
local function nums2(n) local acc = 0 for i = 1, n do local v = N[i % 8 + 1] if v > 3 then acc = acc + v * 2 else acc = acc - v end end return acc end
local function cclo(n) local c = 0 for i = 1, n do local f = (i % 2 == 0) and l1 or gm if i % 3 == 0 then c = c + 1 else c = c + (f ~= nil and 1 or 0) end end return c end
for round = 1, 6 do
  local n = 40 + round * 13
  print(calls(n), calls2(n), nums(n), nums2(n), cclo(n))
end
"#,
    },
    Prog {
        name: "n5_cliff_shape",
        small: false,
        src: r##"
-- N5: the shape of bench/lj/cliff_mixed_strings.lua. `unused` reads strings of both variants and never looks at them (no exit at all:
-- the load guards "some string"); `len` reads `#s` (the variant is observed: the guard fails on 2 of 8 loads, a side trace takes
-- over; the ineffective-trace rule must not flush a root that completes iterations between the hops).
local S = {}
for k = 1, 8 do S[k] = ("x"):rep(34 + k) end
local function unused(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + i % 3 end return acc end
local function len(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + #s end return acc end
local function cnt(...) return select("#", ...) end
local function vararg(n) local acc = 0 for i = 1, n do local s = S[i % 8 + 1] acc = acc + cnt(1, 2, 3, i % 2) end return acc end
print(unused(20000), len(20000), vararg(20000))
print(unused(30000), len(30000), vararg(30000))
"##,
    },
    Prog {
        name: "n1_alias_basic",
        small: true,
        src: r##"
-- N1: closures called from a loop that capture locals of the loop's own frame (main chunk): read-only, read-write, shared by
-- several closures, nested closures, a table / string / float / nil valued upvalue, direct writes between the calls.
local K = 3
local function addk(x) return x + K end
local c = 0
local function inc() c = c + 1 return c end
local function get() return c end
local shared = 10
local function sh1() shared = shared + 1 return shared end
local function sh2() shared = shared * 2 % 1000 return shared end
local nest = 0
local function outer() local function inner() nest = nest + 1 return nest end return inner() + inner() end
local s = 0
for i = 1, 300 do
  s = s + addk(i)
  local a = inc()
  if i % 3 == 0 then c = c + 10 end
  if i % 7 == 0 then c = c - 4 end
  s = s + a + get() + c
  s = s + sh1() + sh2() + shared
  s = s + outer() + nest
end
print(s, c, shared, nest, K)
-- table valued upvalue replaced by the callee and by the loop
local t = {}
local function put(i) t[#t + 1] = i end
local function reset() t = {n = 0} end
local function bump() t.n = (t.n or 0) + 1 end
local tot = 0
for i = 1, 200 do
  put(i)
  if i % 50 == 0 then reset() end
  bump()
  tot = tot + (t.n or 0) + #t
end
print(tot, #t, t.n)
-- upvalue whose type the callee changes now and then
local v = 1
local function setv(i)
  if i % 40 == 0 then v = "str" .. i elseif i % 25 == 0 then v = i + 0.5 elseif i % 17 == 0 then v = nil elseif i % 13 == 0 then v = true else v = i end
end
local acc = 0
for i = 1, 400 do
  setv(i)
  local x = v
  if type(x) == "number" then acc = acc + x elseif type(x) == "string" then acc = acc + #x elseif x then acc = acc + 1000 else acc = acc - 1 end
end
print(acc, v)
"##,
    },
    Prog {
        name: "n1_alias_frames",
        small: true,
        src: r##"
-- N1: function frames (not the main chunk): the closure created in the loop's frame before the loop, a closure of the same
-- proto from an earlier activation (its upvalue is closed: another identity), the frame left by a return through an exit
-- (the upvalue is closed with the right value), vararg frames, an outer loop variable captured and read by an inner loop,
-- pcall / error boundaries and a coroutine whose body runs the loop.
local saved
local function body(n, ext)
  local c = 100
  local function inner() c = c + 1 return c end
  local s = 0
  for i = 1, n do
    local f = (i % 7 == 0 and ext) or inner
    s = s + f()
    if i % 11 == 0 then c = c + 1000 end
  end
  saved = inner
  return s, c
end
local r1, r2 = body(60, nil)
print(r1, r2)
for k = 1, 4 do print(body(60, saved)) end

-- the same proto through one call site: the callee comes from a table, so the callee-identity guard (the proto) passes for the
-- closure of this activation (upvalue = slot of the frame) and for the closure of an earlier activation (closed upvalue)
local saved2
local function body2(n, ext)
  local c = 5
  local function inner() c = c + 1 return c end
  local F = {inner, ext or inner}
  local s = 0
  for i = 1, n do s = s + F[i % 2 + 1]() end
  saved2 = inner
  return s, c
end
print(body2(50, nil))
for k = 1, 4 do print(body2(50, saved2)) end

local function early(n)
  local x = 0
  local function g() x = x + 2 return x end
  for i = 1, n do
    g()
    if i == n - 5 then return g, x end
  end
end
for k = 1, 5 do
  local g, x = early(90 + k)
  print(x, g(), g())
end

local function va(...)
  local k = 0
  local function add(x) k = k + x return k end
  local m = select("#", ...)
  for i = 1, 120 do add(m + i % 3) end
  return k, ...
end
print(va(1, 2, 3))
print(va())
print(va(nil, nil))

local function ovar(m, n)
  local s = 0
  for i = 1, m do
    local function g() return i * 2 end
    for j = 1, n do s = s + g() end
  end
  return s
end
print(ovar(5, 100), ovar(3, 300))

local function prot(n)
  local c = 0
  local function inc() c = c + 1 if c % 17 == 0 then error("boom" .. c) end return c end
  local ok, bad = 0, 0
  for i = 1, n do
    local a, b = pcall(inc)
    if a then ok = ok + b else bad = bad + 1 end
    if i % 20 == 0 then c = c + 3 end
  end
  return ok, bad, c
end
print(prot(200))
print(prot(120))

local function co_loop(n)
  local c = 0
  local function step() c = c + 1 return c end
  local s = 0
  for i = 1, n do
    s = s + step()
    if i % 25 == 0 then coroutine.yield(s, c) end
  end
  return s, c
end
local co = coroutine.wrap(co_loop)
print(co(100)) print(co()) print(co()) print(co()) print(co())
local shared_c = 0
local w = coroutine.wrap(function() while true do shared_c = shared_c + 1 coroutine.yield() end end)
local z = 0
for i = 1, 100 do w() z = z + shared_c end
print(z, shared_c)
"##,
    },
    Prog {
        name: "n6_dynkey_basic",
        small: true,
        src: r##"
-- N6: `t[k] = (t[k] or 0) + 1` for a computed short-string key (HREFV): many distinct keys, tables of several sizes (hash chains of
-- every depth: more keys than a small node vector has slots), present / nil-valued / removed keys, a number key mixed in.
local function mkkeys(n, pre) local ks = {} for i = 1, n do ks[i] = pre .. i .. "x" end return ks end
local function count(ks, rounds)
  local t = {}
  for i = 1, #ks do t[ks[i]] = 0 end
  for r = 1, rounds do
    for i = 1, #ks do local k = ks[i]; t[k] = (t[k] or 0) + 1 end
  end
  local s, n = 0, 0
  for _, v in pairs(t) do s = s + v n = n + 1 end
  return s, n, t[ks[1]], t[ks[#ks]]
end
for _, n in ipairs{1, 2, 3, 5, 8, 9, 16, 33, 64, 100, 129} do print(n, count(mkkeys(n, "k"), 40)) end
-- the same strings reached through two different tables (two SSA refs, one string), loads and stores interleaved
local A, B = mkkeys(24, "q"), {}
for i = 1, #A do B[i] = A[i] end
local t = {}
for i = 1, #A do t[A[i]] = i end
local acc = 0
for r = 1, 60 do
  for i = 1, #A do
    local a, b = A[i], B[i]
    t[a] = (t[a] or 0) + 1
    acc = acc + t[b]
    t[b] = t[b] - 1
    acc = acc + t[a]
  end
end
print(acc)
-- two different keys, a store to one then a load of the other and back
local u = {}
local K = mkkeys(12, "w")
for i = 1, #K do u[K[i]] = i end
local z = 0
for r = 1, 80 do
  for i = 1, #K - 1 do
    local a, b = K[i], K[i + 1]
    u[a] = r
    local x = u[b]
    u[b] = x + 1
    z = z + u[a] + u[b]
  end
end
print(z)
-- values that become nil (the key stays in its node), then non-nil again, and removed keys read as nil
local v = {}
local V = mkkeys(10, "v")
for i = 1, #V do v[V[i]] = i end
local nils, tot = 0, 0
for r = 1, 90 do
  for i = 1, #V do
    local k = V[i]
    if (r + i) % 7 == 0 then v[k] = nil end
    local x = v[k]
    if x == nil then nils = nils + 1 v[k] = 0 else tot = tot + x v[k] = x + 1 end
  end
end
print(nils, tot)
-- number keys and string keys through one site
local m = {}
local M = {}
for i = 1, 20 do M[i] = (i % 3 == 0) and i or ("m" .. i) end
for i = 1, #M do m[M[i]] = 0 end
for r = 1, 50 do for i = 1, #M do local k = M[i]; m[k] = m[k] + 1 end end
local ms = 0
for _, x in pairs(m) do ms = ms + x end
print(ms)
"##,
    },
    Prog {
        name: "n6_dynkey_shape",
        small: true,
        src: r##"
-- N6: the table changes under the loop: a rehash that grows or shrinks the node vector (the recorded node count is stale), another
-- table with the same keys at other nodes, keys that are long strings, metatables with __index / __newindex appearing for present
-- nil-valued keys, absent keys, a metatable that is added mid-loop.
local K = {}
for i = 1, 40 do K[i] = "key" .. i end
local function fill(n) local t = {} for i = 1, n do t[K[i]] = i end return t end
local function sweep(t, n, rounds)
  local s = 0
  for r = 1, rounds do for i = 1, n do local k = K[i]; local x = t[k]; if x then t[k] = x + 1 s = s + x end end end
  return s
end
-- grow: insert new keys between sweeps
local t = fill(4)
local out = {}
for n = 5, 40 do
  t[K[n]] = n
  out[#out + 1] = sweep(t, n, 3)
end
print(table.concat(out, ","))
-- shrink: remove most keys, then insert (rehash to a smaller vector)
for i = 1, 36 do t[K[i]] = nil end
t.extra1 = 1 t.extra2 = 2 t.extra3 = 3
print(sweep(t, 40, 4))
-- many tables of different sizes through one loop (the lsizenode guard changes per table)
local tabs = {}
for n = 1, 20 do tabs[n] = fill(n) end
local s = 0
for r = 1, 30 do for j = 1, 20 do local tt = tabs[j]; for i = 1, j do local k = K[i]; tt[k] = tt[k] + 1 s = s + tt[k] end end end
print(s)
-- the table is replaced during the loop
local cur = fill(8)
local alt = fill(30)
local w = 0
for r = 1, 200 do
  if r % 50 == 0 then cur, alt = alt, cur end
  for i = 1, 8 do local k = K[i]; cur[k] = cur[k] + 1 w = w + cur[k] end
end
print(w)
-- long strings (> 40 bytes) among short ones: they are not HREFV keys
local LK = {}
for i = 1, 12 do LK[i] = (i % 2 == 0) and ("L" .. ("x"):rep(45) .. i) or ("s" .. i) end
local lt = {}
for i = 1, #LK do lt[LK[i]] = 0 end
for r = 1, 60 do for i = 1, #LK do local k = LK[i]; lt[k] = lt[k] + 1 end end
local lsum = 0
for _, x in pairs(lt) do lsum = lsum + x end
print(lsum)
-- absent keys (nil reads and inserts), then the same keys present
local at = fill(10)
local res = 0
for r = 1, 3 do
  for i = 1, 20 do local k = K[i]; local x = at[k]; if x == nil then at[k] = 100 else at[k] = x + 1 res = res + x end end
end
print(res)
-- __index on the metatable: a present nil-valued key falls through; the metatable is set in the middle of the run
local base = { }
for i = 1, 10 do base[K[i]] = 1000 + i end
local obj = fill(10)
for i = 1, 10, 3 do obj[K[i]] = nil end
local seen = 0
for r = 1, 120 do
  if r == 60 then setmetatable(obj, { __index = base }) end
  for i = 1, 10 do local k = K[i]; local x = obj[k]; seen = seen + (x or -1) end
end
print(seen)
-- __newindex: stores of nil-valued present keys call it (the store is not a raw write)
local log = {}
local nt = fill(10)
for i = 2, 10, 3 do nt[K[i]] = nil end
local proxy = setmetatable({}, { __newindex = function(_, k, v) log[#log + 1] = k end })
local ns = 0
for r = 1, 100 do
  if r == 50 then setmetatable(nt, { __newindex = function(tt, k, v) rawset(tt, k, v) ns = ns + 1 end }) end
  for i = 1, 10 do local k = K[i]; nt[k] = (nt[k] or 0) + 1 end
end
print(ns, #log)
local sum = 0
for _, x in pairs(nt) do sum = sum + x end
print(sum)
"##,
    },
    Prog {
        name: "n6_dynkey_hot",
        small: false,
        src: r##"
-- N6: one trace serves every key: 64 and 200 distinct keys in node vectors of 64 and 256 slots (chains of several nodes: with
-- these loads at least one key per chain sits behind another), 6000 iterations each. No exit per key (a trace per key would be a
-- trace for every distinct key hanging off the previous one's exit).
local keys = {}
for i = 1, 200 do keys[i] = "k" .. i .. "x" end
-- (the tables are built by a constructor: inserting distinct keys in a loop is a different matter)
local function mk(n) local p = {} for i = 1, n do p[i] = keys[i] .. "=0" end return load("return {" .. table.concat(p, ",") .. "}")() end
local t = mk(64)
for i = 0, 6000 do local k = keys[i % 64 + 1]; t[k] = (t[k] or 0) + 1 end
local u = mk(200)
for i = 0, 6000 do local k = keys[i % 200 + 1]; u[k] = (u[k] or 0) + 1 end
local a, b = 0, 0
for _, v in pairs(t) do a = a + v end
for _, v in pairs(u) do b = b + v end
print(a, b, t[keys[1]], u[keys[200]])
"##,
    },
    Prog {
        name: "n7_gfor_nest",
        small: true,
        src: r##"
-- N7: `ipairs(t)` / `pairs(t)` called inside a traced loop (the CALL, TFORPREP and the end of the traversal are recorded): inner
-- tables of 0..5, 10, 100 elements, holes, array + hash, nil in the middle, a table that is mutated while it is traversed.
local function mk(n) local t = {} for i = 1, n do t[i] = i * 3 end return t end
local sizes = {0, 1, 2, 3, 4, 5, 10, 100}
local tabs = {}
for i, n in ipairs(sizes) do tabs[i] = mk(n) end
local acc, accp, cnt = 0, 0, 0
for r = 1, 300 do
  for si = 1, #tabs do
    local t = tabs[si]
    for i, v in ipairs(t) do acc = acc + v * i end
    for k, v in pairs(t) do accp = accp + v + k end
    for _ in ipairs(t) do cnt = cnt + 1 end
  end
end
print(acc, accp, cnt)
-- holes, nil in the middle, array + hash, hash only, a table that grew from the hash part
local mixed = { {1, 2, nil, 4, 5}, {nil, nil, 3}, {1, nil}, {10, 20, x = 5, y = 6}, {[1] = 1, [2] = 2, [4] = 4}, {[3] = 3}, {}, {n = 1},
  (function() local t = {} for i = 1, 6 do t[i] = i end t[8] = 8 t[3] = nil return t end)() }
local s1, s2, n1, n2 = 0, 0, 0, 0
for r = 1, 250 do
  for j = 1, #mixed do
    local t = mixed[j]
    for i, v in ipairs(t) do s1 = s1 + i + v n1 = n1 + 1 end
    for k, v in pairs(t) do s2 = s2 + v + (type(k) == "number" and k or 100) n2 = n2 + 1 end
  end
end
print(s1, s2, n1, n2)
-- assigning existing fields while traversing is allowed; so is clearing them
local m1, m2 = 0, 0
for r = 1, 200 do
  local t = {10, 20, 30, a = 1, b = 2, c = 3}
  for k, v in pairs(t) do t[k] = v + 1 end
  for k, v in pairs(t) do if type(k) == "string" then t[k] = nil end end
  for i, v in ipairs(t) do t[i] = v * 2 m1 = m1 + v end
  for k, v in pairs(t) do m2 = m2 + v end
end
print(m1, m2)
-- adding keys while traversing is undefined: it must not crash (only whether it ended in an error is printed)
local u1, u2 = 0, 0
for r = 1, 120 do
  local t = {1, 2, 3, x = 1}
  local ok = pcall(function() for k, v in pairs(t) do if k == 1 then t.new = r t[r + 10] = r end end end)
  u1 = u1 + (ok and 1 or 0)
  local t2 = {1, 2, 3}
  for i, v in ipairs(t2) do if i == 3 and #t2 < 8 then t2[#t2 + 1] = v + 1 end u2 = u2 + v end
end
print(u1, u2)
-- large tables, array and hash
local big, bigh = mk(3000), {}
for i = 1, 600 do bigh["k" .. i] = i end
local bs = 0
for r = 1, 12 do
  for i, v in ipairs(big) do bs = bs + v end
  for k, v in pairs(big) do bs = bs - k end
  for k, v in pairs(bigh) do bs = bs + v end
end
print(bs)
-- the loop leaves early: break, goto, return from a function, a nested ipairs inside a pairs
local function first_big(t, lim) for i, v in ipairs(t) do if v > lim then return i, v end end return -1 end
local br = 0
for r = 1, 200 do
  for i, v in ipairs(tabs[8]) do if v > 50 then br = br + i break end end
  for k, v in pairs(tabs[6]) do if k == 3 then goto out end br = br + 1 end
  ::out::
  br = br + first_big(tabs[7], r % 30) + first_big(tabs[2], 100)
  for i, row in ipairs(mixed) do for j, v in ipairs(row) do br = br + j end end
end
print(br)
-- elements of another type (or nil) appear after the traversal became hot: the exit of the loop's trace is shared by "the table ends"
-- and "the element is not an integer", the side trace behind the loop must tell them apart
local ch = {1, 2, 3, 4, 5, 6}
local cs = 0
for r = 1, 400 do
  if r == 120 then ch[4] = "four" elseif r == 160 then ch[4] = 4.5 elseif r == 200 then ch[4] = nil elseif r == 240 then ch[4] = 4 ch[7] = 7
  elseif r == 300 then ch[2] = false elseif r == 340 then ch[2] = 2 ch[7] = nil end
  for i, v in ipairs(ch) do cs = cs + i end
  for k, v in pairs(ch) do cs = cs + (type(v) == "number" and v or 100) end
end
print(cs)
"##,
    },
    Prog {
        name: "n7_gfor_meta",
        small: true,
        src: r##"
-- N7: metatables and non-tables: `ipairs` honours `__index`, `pairs` honours `__pairs`, the callee may be shadowed, the arguments may be
-- anything (the error messages are the interpreter's), and the call is used for its results in all the ways a call can be.
local acc = 0
-- __index on the table for ipairs: a function, a table, appearing and disappearing in the middle of a hot loop
local fidx = setmetatable({}, {__index = function(t, i) if i <= 5 then return i * 2 end end})
local tidx = setmetatable({1, 2}, {__index = {nil, nil, 30, 40}})
local plain = {7, 8, 9}
local flip = {1, 2, 3, 4}
for r = 1, 300 do
  for i, v in ipairs(fidx) do acc = acc + i * v end
  for i, v in ipairs(tidx) do acc = acc + i + v end
  if r % 40 == 20 then setmetatable(flip, {__index = function(t, i) if i < 7 then return i end end}) end
  if r % 40 == 0 then setmetatable(flip, nil) end
  for i, v in ipairs(flip) do acc = acc + v end
  for i, v in ipairs(plain) do acc = acc + v end
end
print(acc)
-- `__pairs` appearing and disappearing while the loop is hot
local t = {1, 2, 3, x = 10}
local calls = 0
local mt = {__pairs = function(tt) calls = calls + 1 return function(_, k) local nk, nv = next(tt, k) if nk ~= nil then return nk, nv * 2 end end, tt, nil end}
local ps = 0
for r = 1, 400 do
  if r % 60 == 30 then setmetatable(t, mt) elseif r % 60 == 0 then setmetatable(t, nil) end
  for k, v in pairs(t) do ps = ps + v end
end
print(ps, calls)
-- a table with a metatable that has no __pairs
local obj = setmetatable({5, 6, 7, name = 3}, {__index = {z = 1}})
local os = 0
for r = 1, 300 do for k, v in pairs(obj) do os = os + v end for i, v in ipairs(obj) do os = os + i end end
print(os)
-- a metatable with `__index` installed by the body during the last iteration the table has (the traversal ends, but `t[5]` is not nil)
local chg = {1, 2, 3, 4}
local cm = {__index = function(t, i) if i < 7 then return i * 100 end end}
local cg = 0
for r = 1, 300 do
  setmetatable(chg, nil)
  for i, v in ipairs(chg) do cg = cg + v if i == 4 and r % 30 == 29 then setmetatable(chg, cm) end end
end
print(cg)
-- shadowed: a local, a global replaced in the middle of the loop, an iterator that is not the builtin one
local function run(ip, pa, n)
  local s = 0
  for r = 1, n do
    for i, v in ip({4, 5, 6}) do s = s + i * v end
    for k, v in pa({4, 5, 6}) do s = s + k * v end
  end
  return s
end
local function my_ip(t) return function(_, i) i = i + 1 if i <= 2 then return i, i * 7 end end, t, 0 end
local function my_pa(t) return function(_, k) if not k then return 1, 100 end end, t, nil end
print(run(ipairs, pairs, 200), run(my_ip, my_pa, 200))
local saved_ipairs, saved_pairs = ipairs, pairs
local gs = 0
for r = 1, 300 do
  if r == 100 then ipairs = function(t) return saved_ipairs({1, 1}) end end
  if r == 200 then pairs = function(t) return saved_pairs({[1] = 5}) end end
  for i, v in ipairs({4, 5, 6}) do gs = gs + v end
  for k, v in pairs({4, 5, 6}) do gs = gs + v end
end
ipairs, pairs = saved_ipairs, saved_pairs
print(gs)
-- non-table arguments: errors raised at the call or at the first iteration, same text as the interpreter's
local errs = {}
for r = 1, 150 do
  local e1 = select(2, pcall(function() for i, v in ipairs(nil) do end end))
  local e2 = select(2, pcall(function() for k, v in pairs(nil) do end end))
  local e3 = select(2, pcall(function() for k, v in pairs(r) do end end))
  local e4 = select(2, pcall(function() for k, v in ipairs(r) do end end))
  local e5 = select(2, pcall(function() for k, v in ipairs() do end end))
  local e6 = select(2, pcall(function() for k, v in pairs() do end end))
  local e7 = select(2, pcall(function() for k, v in ipairs("abc") do end return "fine" end))
  local e8 = select(2, pcall(function() for k, v in pairs(true) do end end))
  if r % 50 == 1 then errs[#errs + 1] = table.concat({tostring(e1), tostring(e2), tostring(e3), tostring(e4), tostring(e5), tostring(e6), tostring(e7), tostring(e8)}, " | ") end
end
for _, e in ipairs(errs) do print(e) end
-- the results of the call in every position
local tt = {5, 6, 7}
local rs = 0
for r = 1, 300 do
  local a, b, c = ipairs(tt)
  local p, q, w, x = pairs(tt)
  local n1 = select('#', ipairs(tt))
  local n2 = select('#', pairs(tt))
  local pk = table.pack(ipairs(tt)).n + table.pack(pairs(tt)).n
  local function cnt(...) return select('#', ...) end
  local n3 = cnt(ipairs(tt)) + cnt(pairs(tt)) + cnt((ipairs(tt))) + cnt(ipairs(tt), 1) + cnt(1, pairs(tt))
  local a1 = ipairs(tt)
  rs = rs + (b == tt and 1 or 0) + (c == 0 and 1 or 0) + (q == tt and 1 or 0) + (w == nil and 1 or 0) + n1 * 10 + n2 * 100 + pk + n3
    + (type(a) == "function" and 1 or 0) + (p == next and 1 or 0) + (a1 == a and 1 or 0) + (x == nil and 1 or 0)
  for i, v in a, b, c do rs = rs + v end
  for i, v in a, b, 2 do rs = rs + v end
  for k, v in p, q, w do rs = rs + v end
  for k, v in p, q, 2 do rs = rs + v end
  for i, v in ipairs(tt, "extra") do rs = rs + i end
end
print(rs)
"##,
    },
    Prog {
        name: "n7_gfor_misc",
        small: true,
        src: r##"
-- N7: generic `for` loops created in a hot loop in company: coroutine iterators, closures capturing the loop variables, to-be-closed
-- closing values, a Lua iterator, loops nested three deep, a loop in a function called from the loop.
local acc = 0
local function gen(n) return coroutine.wrap(function() for i = 1, n do coroutine.yield(i, i * i) end end) end
for r = 1, 150 do
  for i, sq in gen(3) do acc = acc + sq end
  for i, v in ipairs({1, 2, 3}) do acc = acc + v end
end
print(acc)
local co = coroutine.wrap(function()
  local t = {1, 2, 3}
  for r = 1, 200 do for i, v in ipairs(t) do coroutine.yield(v) end for k, v in pairs(t) do coroutine.yield(k) end end
  return nil
end)
local cs = 0
while true do local v = co() if v == nil then break end cs = cs + v end
print(cs)
-- closures capture the loop variables (a fresh one per iteration)
local fs = {}
local cl = 0
for r = 1, 100 do
  for i, v in ipairs({r, r + 1, r + 2}) do fs[#fs + 1] = function() return i + v end end
  for k, v in pairs({r}) do fs[#fs + 1] = function() return k * v end end
end
for _, f in ipairs(fs) do cl = cl + f() end
print(#fs, cl)
-- the closing value of a generic for (4th value)
local closed = 0
local tbc = setmetatable({}, {__close = function() closed = closed + 1 end})
local tt = {1, 2, 3}
local ts = 0
for r = 1, 150 do
  local a, b, c = ipairs(tt)
  for i, v in a, b, c, tbc do ts = ts + v end
  for i, v in ipairs(tt) do ts = ts + i end
  for k, v in next, tt, nil, nil do ts = ts + k end
end
print(ts, closed)
-- three deep, with the inner tables built in the loop
local d = 0
for r = 1, 60 do
  local rows = {}
  for i = 1, 4 do rows[i] = {} for j = 1, 4 do rows[i][j] = {i, j, r} end end
  for i, row in ipairs(rows) do for j, cell in ipairs(row) do for k, x in ipairs(cell) do d = d + x end end end
  for i, row in pairs(rows) do for j, cell in pairs(row) do for k, x in pairs(cell) do d = d - k end end end
end
print(d)
-- the loop in a function called from the loop; recursion through a generic for
local function sum(t) local s = 0 for i, v in ipairs(t) do s = s + (type(v) == "table" and sum(v) or v) end return s end
local nested = {1, {2, 3, {4, 5}}, 6, {}, {{{7}}}}
local ns = 0
for r = 1, 200 do ns = ns + sum(nested) end
print(ns)
-- an empty table, a one-element table, then the table the traversal started on is replaced inside the loop
local es = 0
local cur = {}
for r = 1, 300 do
  if r % 3 == 0 then cur = {r} elseif r % 3 == 1 then cur = {} else cur = {r, r, r} end
  for i, v in ipairs(cur) do es = es + v end
  for k, v in pairs(cur) do es = es + k end
end
print(es)
-- integer keys beyond the array part and negative / float keys
local odd = {[0] = 1, [-1] = 2, [1.5] = 3, [2^53] = 4, [1] = 5, [2] = 6}
local os = 0
for r = 1, 200 do for k, v in pairs(odd) do os = os + v end for i, v in ipairs(odd) do os = os + i * v end end
print(os)
"##,
    },
    Prog {
        name: "n7_nest_hot",
        small: true,
        src: r##"
-- N7: the plain nest `for r ... do for i, v in ipairs(t) do ... end end` (and pairs): both stay in traces, no exit per outer iteration.
local t = {}
for j = 1, 10 do t[j] = j end
local acc = 0
for r = 1, 3000 do for i, v in ipairs(t) do acc = acc + v * 2 + i end end
print(acc)
local h = {}
for j = 1, 10 do h["k" .. j] = j end
h.x = 7 h.y = 8 -- (all keys are strings: the type of the key is uniform)
for r = 1, 3000 do for k, v in pairs(h) do acc = acc + v * 2 end end
print(acc)
for r = 1, 3000 do for i, v in ipairs(t) do acc = acc + i end for k, v in pairs(h) do acc = acc - 1 end end
print(acc)
"##,
    },
    Prog {
        name: "n3_mm_arith",
        small: true,
        src: r##"
-- N3 stage 1: arithmetic / unary / length / concatenation metamethods that are Lua closures, called from loops.
local V = {}
local k = 3
V.__add = function(a, b) return (type(a) == "table" and a.x or a) + (type(b) == "table" and b.x or b) end
V.__sub = function(a, b) return (type(a) == "table" and a.x or a) - (type(b) == "table" and b.x or b) end
V.__mul = function(a, b) return (type(a) == "table" and a.x or a) * (type(b) == "table" and b.x or b) end
V.__div = function(a, b) return a.x / b end
V.__mod = function(a, b) return a.x % b end
V.__idiv = function(a, b) return a.x // b end
V.__pow = function(a, b) return a.x ^ b end
V.__unm = function(a) return -a.x - k end
V.__len = function(a) return a.x + a.y end
V.__concat = function(a, b) return (type(a) == "table" and "v" .. a.x or a) .. (type(b) == "table" and "v" .. b.x or b) end
V.__band = function(a, b) return a.x & b end
V.__shl = function(a, b) return a.x << b end
V.__bnot = function(a) return ~a.x end
local function new(x, y) return setmetatable({x = x, y = y}, V) end
local a, b = new(5, 7), new(11, 13)
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + (a + b) + (a - b) * 2 + (a * 3) + (3 * a) + (a + 1.5) + (a / 2) + (b % 4) + (b // 2) + (a ^ 2)
    s = s + (-a) + #a + #b + (1 + a) + (2 - b)
    s = s + (a & 6) + (a << 2) + (~b)
    local c = a .. b
    local d = a .. "s"
    s = s + #c + #d
  end
  return s
end
local function run2(n)
  local s = 0
  for i = 1, n do local e = "s" .. a local f = 5 .. b s = s + #e + #f end
  return s
end
print(run(300))
print(run(7))
print(run2(100))
-- the metamethod returns nothing / several values
V.__add = function(x, y) end
local function nil_result(n)
  local c = 0
  for i = 1, n do if (a + b) == nil then c = c + 1 end end
  return c
end
print(nil_result(100))
V.__add = function(x, y) return x.x, y.x, 99 end
local function multi(n)
  local c = 0
  for i = 1, n do c = c + (a + b) end
  return c
end
print(multi(100))
-- a closure with upvalues, one per object (different closures of the same prototype)
local function mk(c0)
  local t = setmetatable({x = c0}, {__sub = function(p, q) c0 = c0 + 1 return p.x - c0 end})
  return t
end
local o1, o2 = mk(10), mk(100)
local function ups(n)
  local s = 0
  for i = 1, n do s = s + (o1 - 1) + (o2 - 1) end
  return s
end
print(ups(100), ups(100))
"##,
    },
    Prog {
        name: "n3_mm_swap",
        small: true,
        src: r##"
-- N3: everything a trace guards when it calls a metamethod changes after it became hot: the metamethod, the metatable of the operand,
-- the operand kind, the event of the other operand, a number metatable, a removed metamethod (an error).
local V, W = {}, {}
V.__add = function(a, b) return 1 end
W.__add = function(a, b) return 1000 end
local plain = {__index = function() return 5 end}
local objs = {setmetatable({}, V), setmetatable({}, V), setmetatable({}, W), setmetatable({}, V)}
local arr = {}
for i = 1, 100 do arr[i] = setmetatable({}, V) end
arr[80] = {}
local num_mt = {__add = function(a, b) return -1 end}
local function run(n, tag)
  local s = 0
  local o = objs[1]
  for i = 1, n do
    s = s + (o + 1) + (2 + o)
    if i == n // 2 then
      if tag == "replace" then V.__add = function(a, b) return 10 end
      elseif tag == "swapmt" then setmetatable(o, W)
      elseif tag == "other" then o = objs[3]
      elseif tag == "remove" then V.__add = nil
      elseif tag == "nummt" then debug.setmetatable(0, num_mt)
      elseif tag == "nummt1" then getmetatable(0).__add = num_mt.__add
      elseif tag == "tab" then o = 5
      elseif tag == "nilmt" then setmetatable(o, nil)
      elseif tag == "cfunc" then V.__add = math.max
      elseif tag == "idx" then V.__index = plain
      end
    end
  end
  return s
end
for _, tag in ipairs{"none", "replace", "swapmt", "other", "remove", "nummt", "nummt0", "nummt1", "tab", "nilmt", "cfunc", "idx"} do
  V.__add = function(a, b) return 1 end
  setmetatable(objs[1], V)
  debug.setmetatable(0, nil)
  if tag == "nummt0" then debug.setmetatable(0, num_mt) end
  if tag == "nummt1" then debug.setmetatable(0, {}) end
  local ok, r = pcall(run, 60, tag)
  print(tag, ok, r)
  local ok, r = pcall(run, 60, tag)
  print(tag, ok, r)
end
debug.setmetatable(0, nil)
local function varobj(n)
  local s = 0
  for i = 1, n do s = s + (arr[i] + 1) end
  return s
end
V.__add = function(a, b) return 1 end
print(pcall(varobj, 100))
print(pcall(varobj, 70))
print(pcall(varobj, 100))
-- first operand without the event, second with it; then the first one gets it
local P = {}
local Q = {__add = function(a, b) return 7 end}
local p, q = setmetatable({}, P), setmetatable({}, Q)
local function order(n)
  local s = 0
  for i = 1, n do
    s = s + (p + q) + (q + p)
    if i == n - 5 then P.__add = function() return 100 end end
  end
  return s
end
print(order(50))
P.__add = nil
print(order(50))
-- a nil-valued key in the first operand's metatable
local R = {__add = nil}
R.__add = 1 R.__add = nil
local r = setmetatable({}, R)
local function nilkey(n)
  local s = 0
  for i = 1, n do s = s + (r + q) + (1 + q) end
  return s
end
print(nilkey(50))
"##,
    },
    Prog {
        name: "n3_mm_nummt_late",
        small: true,
        src: r##"
-- N3 (K3b): the number metatable exists but has no `__add` while the loop records `2 + o` (the table operand provides it); `__add` appears in the
-- number metatable in the middle of the hot loop (no `debug.setmetatable`, so no flush): the recording must have guarded or refused the absence.
local V = {__add = function(a, b) return 1 end}
local o = setmetatable({}, V)
local numt = {}
debug.setmetatable(0, numt)
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + (2 + o)
    if i == n - 10 then numt.__add = function(a, b) return 1000 end end
  end
  return s
end
print(run(100))
numt.__add = nil
print(run(100))
debug.setmetatable(0, nil)
"##,
    },
    Prog {
        name: "n3_mm_err",
        small: true,
        src: r##"
-- N3: errors, yields, recursion and the C-stack depth inside metamethod bodies; the interpreter's messages and tracebacks.
local V = {}
V.__add = function(a, b) return a.x + b.x end
V.__len = function(a) return a.x.y end
V.__concat = function(a, b) return a.x .. b.x end
V.__unm = function(a) if a.x > 150 then error("neg " .. a.x) end return -a.x end
local function new(x) return setmetatable({x = x}, V) end
local function run(n, bad)
  local s = 0
  for i = 1, n do
    local a, b = new(i), new(i + 1)
    if i == bad then b.x = nil end
    s = s + (a + b) + -a
  end
  return s
end
print(run(100, 0))
print(pcall(run, 100, 70))
print(pcall(run, 300, 0))
print(select(2, xpcall(run, debug.traceback, 100, 80)))
local function lens(n)
  local t = new(1)
  t.x = {y = 3}
  local s = 0
  for i = 1, n do s = s + #t + (new(i) .. new(i)):len() end
  return s
end
print(pcall(lens, 50))
-- a yield inside a metamethod, resumed later
local Y = {}
Y.__add = function(a, b) local r = coroutine.yield(a.x) return r + b.x end
local y1, y2 = setmetatable({x = 1}, Y), setmetatable({x = 2}, Y)
local co = coroutine.create(function()
  local s = 0
  for i = 1, 200 do s = s + (y1 + y2) end
  return s
end)
local got = 0
local ok, v = coroutine.resume(co)
while coroutine.status(co) == "suspended" do got = got + v ok, v = coroutine.resume(co, 10) end
print(got, ok, v)
-- a metamethod that yields only sometimes
local Z = {}
Z.__add = function(a, b) if a.n % 25 == 0 then coroutine.yield("y") end a.n = a.n + 1 return a.n end
local z = setmetatable({n = 1}, Z)
local co2 = coroutine.wrap(function() local s = 0 for i = 1, 200 do s = s + (z + 1) end return s end)
local ys = 0
local r = co2()
while r == "y" do ys = ys + 1 r = co2() end
print(ys, r)
-- recursion through the metamethod (the same prototype), and a mutual one
local R = {}
R.__add = function(a, b) if b > 0 then return a + (b - 1) end return 0 end
local ro = setmetatable({}, R)
print(pcall(function() local s = 0 for i = 1, 100 do s = s + (ro + 5) end return s end))
local deep = setmetatable({}, {__add = function(a, b) return a + b end})
print((select(2, pcall(function() return deep + 1 end)):gsub("^.-:%d+: ", "")))
-- two nested metamethods (different prototypes), and a metamethod that calls a function that adds
local A, B = {}, {}
A.__add = function(a, b) return (a.inner + 1) + b end
B.__add = function(a, b) return a.v + b end
local ia = setmetatable({v = 4}, B)
local oa = setmetatable({inner = ia}, A)
local function nest(n) local s = 0 for i = 1, n do s = s + (oa + i) end return s end
print(nest(200), nest(10))
-- allocation inside the metamethod and a collection in the middle
local G = {}
G.__add = function(a, b) local t = {a.x, b.x, tostring(a.x) .. tostring(b.x)} return #t[3] + t[1] end
local g1, g2 = setmetatable({x = 12}, G), setmetatable({x = 345}, G)
local function alloc(n) local s = 0 for i = 1, n do s = s + (g1 + g2) if i % 40 == 0 then collectgarbage() end end return s end
print(alloc(200))
"##,
    },
    Prog {
        name: "n3_mm_exit",
        small: true,
        src: r##"
-- N3: exits inside the body of a metamethod frame (the frame is rebuilt with CIST_MMRET and the C-stack count), then the body returns /
-- recurses in the interpreter: leaked or missing nCcalls levels show as a "C stack overflow" at another place.
local E = {}
E.__add = function(a, b)
  a.n = a.n + 1
  if a.n % 2 == 0 then return b + 1 else return b - 1 end
end
local e = setmetatable({n = 0}, E)
local function alt(n)
  local s = 0
  for i = 1, n do s = s + (e + i) end
  return s
end
print(alt(900))
print(alt(901))
local D = {}
local calls = 0
D.__add = function(a, b)
  if a.deep then calls = calls + 1 return a + b end
  return b
end
local d = setmetatable({deep = false}, D)
local function loop(n)
  local s = 0
  for i = 1, n do
    s = s + (d + i)
    if i == n - 3 then d.deep = true end
  end
  return s
end
d.deep = false
local ok, err = pcall(loop, 100)
print(ok, (tostring(err):gsub("^.-:%d+: ", "")), calls)
d.deep = false
calls = 0
local co = coroutine.wrap(function() return pcall(loop, 100) end)
local ok2, err2 = co()
print(ok2, (tostring(err2):gsub("^.-:%d+: ", "")), calls)
-- the body of the metamethod ends in a tail call, and calls a function that raises
local T = {}
local function leaf(x) if x > 450 then error("big " .. x) end return x end
T.__add = function(a, b) return leaf(b) end
local t = setmetatable({}, T)
local function tail(n) local s = 0 for i = 1, n do s = s + (t + i) end return s end
print(tail(100))
print(pcall(tail, 500))
"##,
    },
    Prog {
        name: "n3_mm_depth",
        small: true,
        src: r##"
-- N3: the trace keeps its distance from the C-stack limit: at every depth near it the program behaves as the interpreter (which
-- runs the metamethod through ccall, and raises "C stack overflow", when there is no room for the in-place frame). The depth is built
-- from in-place metamethod frames (each counts one level); the first iteration of the loop calls nothing, so the interpreter's own check
-- does not run before the trace is entered.
local W = {__add = function(a, b) return a.x + b end}
local w = setmetatable({x = 5}, W)
local function loop(n)
  local s = 0
  for i = 1, n do if i > 1 then s = s + (w + i) end end
  return s
end
for i = 1, 6 do loop(100) end
local R = {}
R.__add = function(a, b)
  if b > 0 then return a + (b - 1) end
  return loop(30)
end
local r = setmetatable({}, R)
for d = 170, 215 do
  print(d, pcall(function() return r + d end))
end
"##,
    },
    Prog {
        name: "n3_mm_cmp",
        small: true,
        src: r##"
-- N3 stage 2: comparison metamethods (__eq, __lt, __le) of Lua closures called from loops; conditions, jumps, `and` / `or`, `~=`,
-- immediates, results that are not booleans, tables without any metamethod, identity changing in the middle of the loop.
local V = {}
V.__eq = function(a, b) return a.x == b.x end
V.__lt = function(a, b)
  local x, y = type(a) == "table" and a.x or a, type(b) == "table" and b.x or b
  return x < y
end
V.__le = function(a, b)
  local x, y = type(a) == "table" and a.x or a, type(b) == "table" and b.x or b
  return x <= y
end
local function new(x) return setmetatable({x = x}, V) end
local a, b, c = new(5), new(5), new(9)
local function run(n)
  local s = 0
  for i = 1, n do
    if a == b then s = s + 1 end
    if a ~= c then s = s + 2 end
    if a == c then s = s + 100 end
    if a < c then s = s + 4 end
    if c < a then s = s + 200 end
    if a <= b then s = s + 8 end
    if c <= a then s = s + 300 end
    if a > c then s = s + 400 end
    if c >= a then s = s + 16 end
    if a < 6 then s = s + 32 end
    if a <= 5 then s = s + 64 end
    if a > 4 then s = s + 128 end
    if a >= 6 then s = s + 500 end
    if 3 < a then s = s + 1000 end
    if a < 5.5 then s = s + 2000 end
    local t = (a < c) and "y" or "n"
    s = s + #t
    local e = (a == b) == true
    if e then s = s + 7 end
    local k = i
    while a < c and k > 0 do k = k - 1 s = s + 1 end
  end
  return s
end
print(run(200))
print(run(5))
-- results that are not booleans
local W = {}
W.__eq = function(a, b) return a.r end
W.__lt = function(a, b) return a.r end
local w1, w2 = setmetatable({r = 0}, W), setmetatable({r = 0}, W)
local function truth(n)
  local s = 0
  for i = 1, n do
    if w1 == w2 then s = s + 1 end
    if w1 < w2 then s = s + 2 end
    if w1 ~= w2 then s = s + 4 end
    if i == n // 2 then w1.r = nil w2.r = nil end
    if i == n - 5 then w1.r = false end
  end
  return s
end
print(truth(100), truth(50))
w1.r = "s" w2.r = "s"
print(truth(40))
-- the closure returns nothing
W.__eq = function() end
print(pcall(truth, 60))
-- the first operand decides, then the second
local P, Q = {}, {__eq = function(a, b) return true end}
local p, q, q2 = setmetatable({}, P), setmetatable({}, Q), setmetatable({}, Q)
local function order(n)
  local s = 0
  for i = 1, n do
    if p == q then s = s + 1 end
    if q == p then s = s + 2 end
    if q == q2 then s = s + 4 end
    if p == p then s = s + 8 end
    if i == n - 10 then P.__eq = function() return false end end
  end
  return s
end
print(order(60))
-- tables without any metamethod, and the identity changing in the middle of the loop
local plain1, plain2 = {}, {}
local cur = plain1
local function ident(n)
  local s = 0
  for i = 1, n do
    if cur == plain1 then s = s + 1 end
    if cur ~= plain2 then s = s + 2 end
    if i % 20 == 0 then cur = (cur == plain1) and plain2 or plain1 end
  end
  return s
end
print(ident(100), ident(100))
-- a metamethod appearing on one of them in the middle of the loop
local late = setmetatable({}, {})
local function appear(n)
  local s = 0
  for i = 1, n do
    if late == plain1 then s = s + 1 end
    if i == n // 2 then getmetatable(late).__eq = function() return true end end
  end
  return s
end
print(appear(60))
getmetatable(late).__eq = nil
print(appear(60))
-- a different table in the first operand with a metatable that has __eq nil-valued
local nv = setmetatable({}, {__eq = 1})
getmetatable(nv).__eq = nil
local function nilval(n) local s = 0 for i = 1, n do if nv == plain1 then s = s + 1 else s = s + 2 end end return s end
print(nilval(50))
"##,
    },
    Prog {
        name: "n3_mm_idx",
        small: true,
        src: r##"
-- N3 stage 3: `__index` / `__newindex` functions and `__call` of Lua closures, called from loops.
local calls, shadow = 0, nil
local P = {}
P.__index = function(t, k) calls = calls + 1 if type(k) == "string" then return #k end return k * 2 end
P.__newindex = function(t, k, v) calls = calls + 1 shadow = v end
P.__call = function(self, a, b, c) return (a or 0) + (b or 0) + (c or 0) + self.base end
local obj = setmetatable({base = 100}, P)
local ghost = setmetatable({}, P)
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + obj.missing + obj.xy + ghost.longname + ghost[i] + ghost[-i] + ghost[0]
    local m = ghost.upper2
    s = s + m
    obj.fresh = i
    s = s + obj.base
    ghost.z = i
    s = s + obj(1) + obj(1, 2) + obj(1, 2, 3) + ghost(i) * 0
    local r1, r2 = obj(5)
    s = s + r1 + (r2 or 0)
    s = s + select("#", obj(1, 2))
  end
  return s
end
P.__index = function(t, k) calls = calls + 1 if type(k) == "string" then return #k end return k * 2 end
local ok, e = pcall(run, 150)
print(ok, e, calls, shadow, rawget(obj, "fresh"))
local ok, e = pcall(run, 40)
print(ok, e, calls)
-- the metamethod / metatable / table state changes after the loops were recorded
local Q = {}
Q.__index = function(t, k) return 1 end
Q.__newindex = function(t, k, v) rawset(t, k, v) end
Q.__call = function(self, x) return x end
local q = setmetatable({}, Q)
local function swap(n, tag)
  local s = 0
  for i = 1, n do
    s = s + q.a + q[i] + q(i)
    q.b = i
    q.b = nil
    if i == n // 2 then
      if tag == "index" then Q.__index = function(t, k) return 5 end
      elseif tag == "tab" then Q.__index = {a = 3}
      elseif tag == "call" then Q.__call = function(self, x) return -x end
      elseif tag == "nocall" then Q.__call = nil
      elseif tag == "newindex" then Q.__newindex = nil
      elseif tag == "set" then rawset(q, "a", 9) rawset(q, 3, 7)
      elseif tag == "arr" then for j = 1, 100 do rawset(q, j, j) end
      elseif tag == "mt" then setmetatable(q, nil)
      elseif tag == "cfunc" then Q.__index = rawget
      end
    end
  end
  return s
end
for _, tag in ipairs{"none", "index", "tab", "call", "nocall", "newindex", "set", "arr", "mt", "cfunc"} do
  Q.__index = function(t, k) return 1 end
  Q.__newindex = function(t, k, v) rawset(t, k, v) end
  Q.__call = function(self, x) return x end
  q = setmetatable({}, Q)
  for k in pairs(q) do q[k] = nil end
  print(tag, pcall(swap, 50, tag))
  print(tag, pcall(swap, 50, tag))
end
-- an error inside the metamethod, and a yield
local E = setmetatable({}, {__index = function(t, k) if k > 40 then error("big " .. k) end return k end,
                            __call = function(self, k) if k > 30 then error("call " .. k) end return k end})
local function errs(n) local s = 0 for i = 1, n do s = s + E[i] + E(i) end return s end
print(pcall(errs, 20))
print(pcall(errs, 100))
local co = coroutine.create(function()
  local Y = setmetatable({}, {__index = function(t, k) return coroutine.yield(k) end, __call = function(self, k) return coroutine.yield(-k) end})
  local s = 0
  for i = 1, 60 do s = s + Y[i] + Y(i) end
  return s
end)
local sum = 0
local ok, v = coroutine.resume(co)
while coroutine.status(co) == "suspended" do sum = sum + v ok, v = coroutine.resume(co, 2) end
print(ok, v, sum)
-- the receiver of `__call` is also an argument; a callable that is itself called through a callable
local C1 = setmetatable({}, {__call = function(self, f, ...) return f(...) end})
local function viaf(a, b) return a * b end
local function callp(n) local s = 0 for i = 1, n do s = s + C1(viaf, i, 3) end return s end
print(callp(100), callp(30))
"##,
    },
    Prog {
        name: "nn_tail_call",
        small: true,
        src: r##"
-- BUNDLE-N: `return Class(...)` (a TAILCALL of a table with a Lua `__call`, vararg and fixed callees) and `return f(...)` into a vararg callee.
local Obj = {}
Obj.__index = Obj
function Obj:__call(...) local o = setmetatable({}, self) o:new(...) return o end
function Obj:new(x, y) self.x = x or 0 self.y = y or 0 end
local Vec = setmetatable({}, Obj)
Vec.__index = Vec
Vec.new = Obj.new
Vec.__call = Obj.__call
Vec.__add = function(a, b) return Vec(a.x + b.x, a.y + b.y) end
Vec.__unm = function(a) return Vec(-a.x, -a.y) end
setmetatable(Vec, {__call = Obj.__call})
local Fix = setmetatable({}, {__call = function(self, a, b, c) return (a or 0) + (b or 0) + (c or 0), 9 end})
local function viaFix(a, b) return Fix(a, b) end
local function va(...) return select('#', ...), ... end
local function tva(a, ...) return va(a, ...) end
local function tva0() return va() end
local function tvaN(n, ...) return va(n, n + 1, ...) end
local function viaVec(a, b) return Vec(a, b) end
local V = Vec(1, 2)
local function run(n, w)
  local acc = 0
  for i = 1, n do
    local p = V + viaVec(i, i * 2)
    local q = -p
    acc = acc + p.x + q.y
    local c, d = viaFix(i, w)
    local k, a1, a2, a3 = tva(i, w, 7)
    local z = tva0()
    local m, b1, b2, b3 = tvaN(i, 5)
    acc = acc + c + d + k + a1 + a2 + a3 + z + m + b1 + b2 + b3
  end
  return acc
end
print(run(300, 3), run(40, 4))
-- a callee that changes: __call replaced mid-run, a missing __call, a non-function __call
local C = setmetatable({}, {__call = function(self, x) return x + 1 end})
local function tc(x) return C(x) end
local function loop(n) local s = 0 for i = 1, n do s = s + tc(i) end return s end
print(loop(100))
getmetatable(C).__call = function(self, x) return x * 2 end
print(loop(100))
getmetatable(C).__call = nil
print(pcall(loop, 10))
getmetatable(C).__call = 5
print(pcall(loop, 10))
getmetatable(C).__call = setmetatable({}, {__call = function(self, a, b) return 100 + (b or 0) end})
print(pcall(loop, 10))
local function err(...) error("boom" .. select('#', ...)) end
local function te(a, b) return err(a, b) end
for i = 1, 100 do local ok, e = pcall(te, i, 2) if i % 50 == 0 then print(ok, e) end end
print(select(2, pcall(function() return setmetatable({}, {__call = function(...) return debug.traceback("x", 1) end})(1) end)):gsub("\n.*", ""))
"##,
    },
    Prog {
        name: "nn_idx_chain",
        small: true,
        src: r##"
-- BUNDLE-N: `__index` Lua functions behind tables (chain depth 2 and 3), the same prototype twice on the frame stack (a subclass's index
-- function calling the parent's), `rawget(t, "str")`, and metatable / chain changes between iterations.
local function mk(parent)
  local dict = {}
  local static = setmetatable({}, {__index = function(_, k)
    local r = rawget(dict, k)
    if r == nil and parent then return parent.static[k] end
    return r
  end})
  return {dict = dict, static = static, parent = parent}
end
local A = mk(nil)
local B = mk(A)
local C = mk(B)
A.dict.hello = "a-hello" A.dict.x = 1
B.dict.hello = "b-hello"
C.dict.y = 2
local clsA = setmetatable({}, {__index = A.static})
local clsB = setmetatable({}, {__index = B.static})
local clsC = setmetatable({}, {__index = C.static})
local mid = setmetatable({}, {__index = clsC})
local top = setmetatable({}, {__index = mid})
local function look(n, o)
  local c = 0
  for i = 1, n do
    local h, x, y, z = o.hello, o.x, o.y, o.zzz
    if h then c = c + #h end
    c = c + (x or 0) + (y or 0) + (z and 100 or 0)
  end
  return c
end
print(look(200, clsA), look(200, clsB), look(200, clsC), look(200, mid), look(200, top))
A.dict.zzz = true
print(look(100, top), look(100, clsB))
A.dict.zzz = nil
rawset(top, "y", 50) rawset(mid, "x", 7)
print(look(100, top), look(100, mid))
top.y = nil
setmetatable(mid, {__index = clsB})
print(look(100, top))
setmetatable(mid, {__index = function(_, k) return k == "x" and 1000 or nil end})
print(look(100, top))
setmetatable(mid, nil)
print(look(100, top))
-- rawget with a string key: present, absent, nil-valued, dynamic keys, with and without a metatable
local P = setmetatable({a = 1, b = false}, {__index = function() return "idx" end})
local keys = {"a", "b", "c", "a", "d", "b"}
local function rg(n)
  local s = 0
  for i = 1, n do
    local k = keys[i % #keys + 1]
    local v = rawget(P, k)
    if v == nil then s = s + 1 elseif v == false then s = s + 10 else s = s + v * 100 end
    if i == n // 2 then P.c = 5 P.d = nil end
  end
  return s
end
print(rg(60), rg(60))
P.c = nil
print(rg(60))
print(pcall(function() for i = 1, 50 do rawget(nil, "a") end end))
print(pcall(function() for i = 1, 50 do rawget(P, i % 2 == 0 and "a" or {}) end end))
-- an __index function that is the same prototype at two levels with different upvalues (recursion unrolled), erroring at the end
local function level(n, base)
  if n == 0 then return base end
  return setmetatable({}, {__index = function(t, k) if k == "boom" then error("boom " .. n) end return level(0, nil) and nil or base[k] end})
end
local o0 = {v = 3}
local o1 = level(1, o0) local o2 = level(2, o1) local o3 = level(3, o2)
local function lv(n, o) local s = 0 for i = 1, n do s = s + (o.v or 0) end return s end
print(lv(80, o3), lv(80, o2))
print(pcall(function() for i = 1, 80 do local _ = o3.boom end end))
"##,
    },
    Prog {
        name: "po_poly",
        small: true,
        src: r##"
-- BUNDLE-O: polymorphic call sites. Method calls through `__index` on receivers of 2, 3, 4 and 5 classes (five different prototypes), a set of
-- classes that changes over time, a method replaced in the middle of a run, closures of ONE prototype with different upvalues (the callee guard
-- compares prototypes, the upvalue reads go through the closure), and method lookups at different depths of an `__index` chain.
local seed = 12345
local function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648; return seed % n + 1 end
local function m1(self, x) self.v = self.v + x return self.v end
local function m2(self, x) self.v = self.v - x * 2 return self.v end
local function m3(self, x) local t = self.v * 3 + x self.v = t % 1000 return t end
local function m4(self, x) self.w = (self.w or 0) + 1 self.v = self.v + self.w return x end
local function m5(self, x) if x % 2 == 0 then return self.v end self.v = self.v + 1 return -x end
local protos = {m1, m2, m3, m4, m5}
local classes = {}
for i = 1, 5 do
  local c = {name = "C" .. i}
  c.f = protos[i]
  c.__index = c
  classes[i] = c
end
local function obj(c, v) return setmetatable({v = v}, c) end
local function run(objs, n, pick)
  local s = 0
  for i = 1, n do
    local o = objs[pick(i)]
    s = s + o:f(i)
  end
  local sum = 0
  for _, o in ipairs(objs) do sum = sum + o.v end
  return s, sum
end
-- 2, 3, 4, 5 receiver classes; round robin and random
for k = 2, 5 do
  local objs = {}
  for j = 1, k do objs[j] = obj(classes[j], j * 10) end
  print(k, "rr", run(objs, 600, function(i) return (i - 1) % k + 1 end))
  print(k, "rnd", run(objs, 600, function(i) return rnd(k) end))
end
-- the set of classes changes over time
do
  local objs = {}
  for j = 1, 5 do objs[j] = obj(classes[j], j) end
  for phase = 1, 8 do
    local lo, hi = (phase % 3) + 1, (phase % 3) + 3
    print("phase", phase, run(objs, 300, function(i) return lo + (i % (hi - lo + 1)) end))
  end
end
-- a method replaced in the middle of the run (and put back)
do
  local objs = {}
  for j = 1, 4 do objs[j] = obj(classes[j], j) end
  local function pick(i) return (i * 7) % 4 + 1 end
  print("repl", run(objs, 200, pick))
  classes[2].f = m5
  print("repl", run(objs, 200, pick))
  classes[3].f = nil
  print("repl", pcall(run, objs, 200, pick))
  classes[3].f = m3
  classes[2].f = m2
  print("repl", run(objs, 200, pick))
end
-- closures of one prototype, different upvalues, and one foreign prototype among them
do
  local function mk(d, r) return function(self, x) self.v = self.v + d * x - r return self.v end end
  local ms = {mk(1, 0), mk(2, 1), mk(-3, 5), mk(10, 7)}
  local objs = {}
  for j = 1, 5 do objs[j] = {v = j, f = ms[j] or m1} end
  print("clo", run(objs, 500, function(i) return rnd(5) end))
  ms[2] = mk(100, 3) objs[2].f = ms[2]
  print("clo", run(objs, 500, function(i) return (i % 5) + 1 end))
end
-- method lookup at different depths of an `__index` chain: instances of 5 classes, the method defined in the class, its parent or the root
do
  local root = {f = m1} root.__index = root
  local mid = setmetatable({}, root) mid.__index = mid
  local low = setmetatable({}, mid) low.__index = low
  local own = setmetatable({f = m2}, mid) own.__index = own
  local own2 = setmetatable({f = m3}, low) own2.__index = own2
  local cs = {root, mid, low, own, own2}
  local objs = {}
  for j = 1, 5 do objs[j] = setmetatable({v = j}, cs[j]) end
  print("chain", run(objs, 600, function(i) return rnd(5) end))
  mid.f = m4
  print("chain", run(objs, 600, function(i) return (i % 5) + 1 end))
  setmetatable(mid, {__index = classes[5]})
  print("chain", run(objs, 600, function(i) return rnd(5) end))
end
"##,
    },
    Prog {
        name: "s_laprune",
        small: true,
        src: r##"
-- BUNDLE-P (`laprune`): the root trace (the outer `for`) unrolls no callee loop itself; only the side traces taken at the cold branch of `step` unroll
-- the data-dependent `while` of `get` (a BST lookup, trip count = depth of the key). The `loopany` guard blocks the loop and must flush only those
-- side traces: the root stays. (Shrunk from awfy/cd's `RBT:get` under `recurse`.)
-- loopany guard repro: the root trace of the outer loop never unrolls a callee loop itself; only side traces (from a
-- branch inside an inlined callee) unroll a BST lookup whose trip count is the (data-dependent) depth of the key.
local function newnode(k) return { k = k, v = k * 2, l = nil, r = nil } end
local function insert(t, k)
  if t.root == nil then t.root = newnode(k) return end
  local x = t.root
  while true do
    if k < x.k then if x.l then x = x.l else x.l = newnode(k) return end
    else if x.r then x = x.r else x.r = newnode(k) return end end
  end
end
local function get(t, k)
  local x = t.root
  while x ~= nil do
    if k < x.k then x = x.l elseif k > x.k then x = x.r else return x.v end
  end
  return 0
end
local function step(t, k)
  -- the callee frame: most calls take the cheap path, every 8th looks the key up
  if k % 8 ~= 0 then return k % 7 end
  return get(t, k)
end
local tree = { root = nil }
local seed, keys = 12345, {}
for i = 1, 512 do seed = (seed * 1103515245 + 12345) % 2147483648; keys[i] = seed % 100000; insert(tree, keys[i]) end
local s = 0
for it = 1, 40 do
  for i = 1, #keys do s = s + step(tree, keys[i]) end
end
print(s)
"##,
    },
    Prog {
        name: "s_loopany",
        small: true,
        src: r##"
-- BUNDLE-S: `while true` loops of inlined callees (`loopany`): `sift` leaves after a pseudo-random number of iterations (every distinct count
-- is another exit of the unrolled trace: the guard blocks the loop after a few side attempts), `fixed` always after three (stays unrolled), and a
-- `repeat` callee whose count follows the caller's counter.
local seed = 7
local function rnd() seed = (seed * 1103515245 + 12345) % 2147483648; return seed end
local a = {}
for i = 1, 400 do a[i] = rnd() % 1000 end
local function sift(lo, hi)
  local root = lo
  while true do
    local child = root * 2
    if child > hi then break end
    if child < hi and a[child] < a[child + 1] then child = child + 1 end
    if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
  end
  return root
end
local function fixed(x)
  local k, s = 0, x
  while true do
    k = k + 1
    s = s * 3 % 1009 + k
    if k >= 3 then break end
  end
  return s
end
local function rep(n)
  local c, s = 0, 0
  repeat c = c + 1; s = s + c * n until c >= n % 5 + 1
  return s
end
local t = 0
for round = 1, 12 do
  for i = 200, 1, -1 do t = t + sift(i, 400) + fixed(i) + rep(i + round) end
  for i = 1, 400 do a[i] = rnd() % 1000 end
end
print(t, a[1], a[400])
"##,
    },
    Prog {
        name: "po_branch",
        small: true,
        src: r##"
-- BUNDLE-O: an eight-way branch chain on a data-dependent value: its exits are branch guards, not dispatch guards (no method or field
-- lookup, no callee), so a full family does not take more side traces for them.
local function f(n)
  local s = 0
  for i = 1, n do
    local v = (i * 7 + i // 3) % 8
    if v == 0 then s = s + 1
    elseif v == 1 then s = s + 2
    elseif v == 2 then s = s * 2 % 1000
    elseif v == 3 then s = s - 3
    elseif v == 4 then s = s + i
    elseif v == 5 then s = s - i % 5
    elseif v == 6 then s = s + 6
    else s = s + 7 end
  end
  return s
end
print(f(800), f(800), f(333))
"##,
    },
    Prog {
        name: "n3_mm_idx2",
        small: true,
        src: r##"
-- N3 stage 3: what the `__index` function call of an integer key and the `__newindex` call of a string key guard: the key stays outside
-- the array part, the hash part stays the shared empty node, the key stays absent.
local Q = {__index = function(t, k) return -k end}
local empty = setmetatable({}, Q)
local arr = setmetatable({101, 102, 103, 104, 105, 106, 107, 108}, Q)
local hashed = setmetatable({[5] = 50, [6] = 60}, Q)
local neg = setmetatable({[-3] = 7}, Q)
local pool = {empty, empty, empty, empty, arr, empty, hashed, empty, neg, empty}
local function pick(n, which)
  local s = 0
  for i = 1, n do
    local o = empty
    if i > 20 then o = pool[i % #pool + 1] end
    if which == 1 then o = empty end
    s = s + o[i % 9 + 1] + o[-3] + o[0]
  end
  return s
end
print(pick(300, 0), pick(300, 1), pick(50, 0))
local N = {}
local seen = 0
N.__newindex = function(t, k, v) seen = seen + v end
local g = setmetatable({}, N)
local function put(n, tag)
  for i = 1, n do
    g.z = i
    if i == n // 2 and tag == "present" then rawset(g, "z", 0) end
    if i == n // 2 and tag == "other" then rawset(g, "y", 0) end
    if i == n // 2 and tag == "nilval" then rawset(g, "z", 0) rawset(g, "z", nil) end
  end
  return seen, rawget(g, "z")
end
for _, tag in ipairs{"none", "present", "other", "nilval", "present"} do
  seen = 0
  g = setmetatable({}, N)
  print(tag, pcall(put, 80, tag))
  print(tag, pcall(put, 80, tag))
end
"##,
    },
    Prog {
        name: "n3b_setmt",
        small: true,
        src: r##"
-- N3b: setmetatable(t, mt) in traces: a fresh table with a metatable, returned (tail call), re-set, cleared, __gc.
local V = {}
V.__index = V
function V.sum(o) return o.x + o.y end
local W = {__index = {x = 100, y = 200, w = 5}}
local function new(x, y) return setmetatable({x = x, y = y}, V) end
local function tail(x, y) return setmetatable({x = x, y = y}, V) end
local function viaw(x) return setmetatable({x = x}, W) end
local function run(n)
  local s = 0
  for i = 1, n do
    local o = new(i, 2)
    local p = tail(1, i)
    local q = viaw(i)
    s = s + o:sum() + p:sum() + q.y + q.w + (getmetatable(o) == V and 1 or 0)
  end
  return s
end
print(run(300), run(7), run(300))
-- the returned value is the table itself; repeated and nil metatables on one object
local function again(n)
  local t = {x = 1, y = 2}
  local s = 0
  for i = 1, n do
    local r = setmetatable(t, i % 3 == 0 and nil or (i % 3 == 1 and V or W))
    if r ~= t then s = s + 1000 end
    s = s + (t.w or 0) + (rawget(t, "x") or 0)
    if i % 3 ~= 0 then s = s + t.y end
  end
  return s, getmetatable(t) == W
end
print(again(300))
print(again(7))
-- a table as its own metatable, a metatable with an __index function, the metatable changed under the loop
local self_t = {x = 3}
self_t.__index = self_t
local function selfmt(n)
  local s = 0
  for i = 1, n do
    local o = setmetatable({}, self_t)
    s = s + o.x
    if i == n // 2 then self_t.x = 40 end
  end
  return s
end
print(selfmt(200))
local calls = 0
local F = {__index = function(t, k) calls = calls + 1 return k end}
local function idxf(n)
  local s = 0
  for i = 1, n do local o = setmetatable({}, F) s = s + #o.abc end
  return s, calls
end
print(idxf(100))
-- the second argument changes after the loop is hot: nil, a metatable with __gc
local function mk(m) return setmetatable({}, m) end
local G = {}
local gced = 0
local function kinds(n, m)
  local s = 0
  for i = 1, n do
    local o = mk(m)
    s = s + (getmetatable(o) == m and 1 or 0)
  end
  return s
end
print(kinds(200, V), kinds(200, nil), kinds(50, V))
G.__gc = function() gced = gced + 1 end
print(kinds(300, G))
collectgarbage() collectgarbage()
print(gced)
print(kinds(50, V))
local H = {}
local hgc = 0
local function fill(n, m)
  for i = 1, n do setmetatable({}, m) end
end
fill(300, H) fill(300, H)
H.__gc = function() hgc = hgc + 1 end
fill(300, H)
collectgarbage() collectgarbage()
print(hgc)
"##,
    },
    Prog {
        name: "n3b_setmt_err",
        small: true,
        src: r##"
-- N3b: errors and protected metatables raised by the interpreter's setmetatable after traces were recorded.
local V = {__index = {z = 1}}
local P = {__metatable = "locked"}
local Q = {__metatable = false}
local function set(t, m) return setmetatable(t, m) end
local function loop(n, objs, m)
  local s = 0
  for i = 1, n do
    local o = objs[i % #objs + 1]
    local ok, e = pcall(set, o, m)
    if ok then s = s + 1 else s = s + #tostring(e) end
  end
  return s
end
local plain = {{}, {}, {}}
print(loop(300, plain, V))
print(loop(300, plain, V))
local mixed = {{}, setmetatable({}, P), setmetatable({}, Q), setmetatable({}, V)}
print(loop(300, mixed, V))
print(pcall(set, setmetatable({}, P), V))
print(pcall(set, setmetatable({}, Q), nil))
print(pcall(set, 1, V))
print(pcall(set, "x", V))
print(pcall(set, {}, 1))
print(pcall(set, {}, "x"))
print((pcall(set, {})))
print(pcall(setmetatable))
print(pcall(set, nil, V))
-- direct (inlined) calls in a hot loop with a changing argument kind
local function direct(n, bad)
  local s = 0
  for i = 1, n do
    local t = {}
    local a = (i == bad) and 7 or V
    local ok, e = pcall(function() return setmetatable(t, a) end)
    s = s + (ok and 1 or #e)
  end
  return s
end
print(direct(100, 0), direct(100, 50), direct(100, 0))
-- a protected metatable on an object met by the hot loop (the error is raised by the interpreter's setmetatable; caught outside the loop)
local function walk(objs, m)
  local n = 0
  for i = 1, #objs do setmetatable(objs[i], m) n = n + 1 end
  return n
end
local function fresh(k, prot)
  local l = {}
  for i = 1, k do l[i] = {} end
  if prot then setmetatable(l[prot], {__metatable = "locked"}) end
  return l
end
print(walk(fresh(300), V), walk(fresh(300), V))
print(pcall(walk, fresh(300, 250), V))
print(pcall(walk, fresh(300, 150), V))
print(walk(fresh(300), V))
-- protection appears on the metatable of the object the loop keeps re-setting
local obj = setmetatable({}, V)
local function reset(n)
  local s = 0
  for i = 1, n do
    local ok, e = pcall(setmetatable, obj, V)
    s = s + (ok and 1 or 100)
    if i == n // 2 then V.__metatable = "now protected" end
  end
  V.__metatable = nil
  return s
end
print(reset(200))
print(reset(200))
"##,
    },
    Prog {
        name: "n3b_setmt_gc",
        small: true,
        src: r##"
-- N3b: the collector barrier of setmetatable: an old (black) table gets a young metatable that nothing else references.
local keep = {}
local function churn(n)
  local x
  for i = 1, n do x = {i, i + 1, i + 2} end
  return x
end
local function swap(t, mts, n)
  for i = 1, n do
    local mt = mts[i]
    mts[i] = false
    setmetatable(t, mt)
  end
  return t
end
local function pre(n)
  local mts = {}
  for i = 1, n do mts[i] = {__index = {v = i}} end
  return mts
end
local function swap3(t, mts, n)
  local junk
  for i = 1, n do
    local mt = mts[i]
    mts[i] = false
    setmetatable(t, mt)
    junk = {i, i, i}
  end
  return t
end
local function barrier3(n)
  local t = {}
  keep[3] = t
  local s = 0
  for r = 1, 40 do
    local mts = pre(n)
    swap3(t, mts, n)
    s = s + t.v
  end
  return s
end
local function barrier2(n)
  local t = {}
  keep[2] = t
  for r = 1, 4 do
    local mts = pre(n)
    swap(t, mts, n)
    collectgarbage("step", 1)
  end
  return t.v
end
local function barrier(n)
  local s = 0
  local t = {}
  keep[1] = t
  for i = 1, n do
    local mt = {__index = {v = i}}
    setmetatable(t, mt)
    churn(5)
    s = s + t.v
    if i % 16 == 0 then collectgarbage("step", 1) end
  end
  return s
end
print(barrier(300))
print(barrier(100))
print(barrier2(100), barrier2(100))
print(barrier3(100), barrier3(100))
collectgarbage()
print(barrier(100))
print(barrier2(100))
-- many objects, each with its own metatable
local function objs(n)
  local list = {}
  for i = 1, n do
    list[i] = setmetatable({id = i}, {__index = function(_, k) return k .. i end})
  end
  local s = 0
  for i = 1, n do s = s + #list[i].k end
  return s
end
print(objs(200))
print(objs(60))
"##,
    },
    Prog {
        name: "n3b_setmt_co",
        small: true,
        src: r##"
-- N3b: setmetatable in coroutines (yields between the call and the use).
local V = {__index = {get = function(self) return self.v end}}
local function gen(n)
  return coroutine.wrap(function()
    for i = 1, n do
      local o = setmetatable({v = i}, V)
      coroutine.yield(o)
    end
  end)
end
local function run(n)
  local s = 0
  local g, h = gen(n), gen(n)
  for i = 1, n do s = s + g():get() + h().v end
  return s
end
print(run(200), run(9))
local function run2(n)
  local s = 0
  for i = 1, n do
    local co = coroutine.create(function(a)
      local o = setmetatable({v = a}, V)
      local b = coroutine.yield(o:get())
      return setmetatable(o, {__index = {get = function() return b end}}):get()
    end)
    local _, x = coroutine.resume(co, i)
    local _, y = coroutine.resume(co, x * 2)
    s = s + x + y
  end
  return s
end
print(run2(100))
"##,
    },
    Prog {
        name: "n3b_mm_alloc",
        small: false,
        src: r##"
-- N3b: a metamethod that allocates (`return setmetatable({..}, V)`): the collector steps every few dozen iterations in a tiny heap,
-- the trace exits at the allocation, the interpreter allocates and steps, the loop must get back into the trace.
local V = {}
V.__index = V
V.__add = function(a, b) return setmetatable({x = a.x + b.x, y = a.y + b.y}, V) end
V.__eq = function(a, b) return a.x == b.x and a.y == b.y end
local one = setmetatable({x = 1, y = 2}, V)
local function run(n)
  local acc = setmetatable({x = 0, y = 0}, V)
  local s = 0
  for i = 1, n do
    acc = acc + one
    if acc == one then s = s + 1 end
  end
  return acc.x, acc.y, s
end
print(run(60000))
print(run(60000))
"##,
    },
    Prog {
        name: "n3b_setmt_gc_exit",
        small: true,
        src: r##"
-- N3b: a metatable that gets a __gc after the loop is hot: the interpreter registers the finalizer (the trace's helper refuses).
local H = {}
local hgc = 0
local function fill(n, m)
  for i = 1, n do setmetatable({}, m) end
end
fill(400, H) fill(400, H)
H.__gc = function() hgc = hgc + 1 end
fill(400, H)
collectgarbage() collectgarbage()
print(hgc)
"##,
    },
    Prog {
        name: "n3b_setmt_gc_exit2",
        small: true,
        src: r##"
-- N3b (K3b): as `n3b_setmt_gc_exit`, but the table escapes into a store, so it is never sunk (with `sinkmt` on, `setmetatable({}, m)` of a
-- fresh table is sunk and the `SETMT` helper does not run): the helper refuses the metatable that got a __gc, every iteration after it leaves the trace.
local H = {}
local hgc = 0
local keep = {}
local function fill(n, m)
  for i = 1, n do keep[i % 8 + 1] = setmetatable({}, m) end
end
fill(400, H) fill(400, H)
H.__gc = function() hgc = hgc + 1 end
fill(400, H)
keep = nil
collectgarbage() collectgarbage()
print(hgc)
"##,
    },
    Prog {
        name: "a1_inline_newref",
        small: true,
        src: r##"
-- A1: constructors with constant string keys: the key insertions of a table the trace allocated are stores at nodes known at record
-- time. Every shape below is looked up through its chains (collisions, the free node of the colliding insertion), traversed in node
-- order (`pairs`) and used as a metatable (the metamethod-absence flags). Shapes over 8 nodes, array parts, values that change
-- type in the middle of a constructor (an exit with the table half built) and collectable values take the other paths.
-- (the string hash seed differs between the states of the test harness: the node order of a table is compared with the order of a
-- table the interpreter built in the same state, not with the order in another run)
local function dump(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) end
  return table.concat(ks, ",")
end
local function build(ctor, look)
  local src = "local keep = {} return function(n) local acc = 0 for i = 1, n do local t = " .. ctor
    .. " keep[i % 4 + 1] = t acc = acc + " .. look .. " end return acc, keep end"
  return assert(load(src))()
end
local shapes = {
  {"{a = i}", "t.a"},
  {"{a = i, b = i + 1}", "t.a + t.b"},
  {"{a = i, b = 2, c = 3}", "t.a + t.b + t.c"},
  {"{alpha = i, beta = 2, gamma = 3, delta = 4}", "t.alpha + t.beta + t.gamma + t.delta"},
  {"{k1 = i, k2 = 2, k3 = 3, k4 = 4, k5 = 5}", "t.k1 + t.k2 + t.k3 + t.k4 + t.k5"},
  {"{name = i, value = 2, key = 3, next = 4, len = 5, size = 6, id = 7, tag = 8}", "t.name + t.value + t.key + t.next + t.len + t.size + t.id + t.tag"},
  {"{x = i, y = 2, z = 3, w = 4, v = 5, u = 6, t = 7, s = 8, r = 9}", "t.x + t.y + t.z + t.w + t.v + t.u + t.t + t.s + t.r"},
  {"{i, i + 1, x = i, y = 2}", "t[1] + t[2] + t.x + t.y"},
  {"{x = 'v' .. (i % 7), y = {i}, z = i * 0.5}", "(t.x and 1 or 0) + t.y[1] + t.z"},
  {"{x = i % 5 == 0 and 0.5 or i, y = 2}", "t.x + t.y"},
  {"{x = i, y = 2, x2 = 3, y2 = 4, x3 = 5, y3 = 6, x4 = 7}", "t.x + t.y + t.x2 + t.y2 + t.x3 + t.y3 + t.x4"},
}
local sum = 0
for si, sh in ipairs(shapes) do
  local f = build(sh[1], sh[2])
  local _, kc = f(4)
  local cold = dump(kc[1])
  local a1, k1 = f(2500)
  local a2, k2 = f(7)
  local same = true
  for j = 1, 4 do same = same and dump(k1[j]) == cold and dump(k2[j]) == cold end
  print(si, a1, a2, same)
end
-- tables used as metatables: the absence flags of a table that got keys must not claim that the events are absent
local base = {foo = "from base"}
local lenf = function() return 42 end
local kept = {}
local function meta(n)
  local acc = 0
  for i = 1, n do
    local mt = {__index = base, __len = lenf}
    kept[i % 3 + 1] = mt
    local o = setmetatable({}, mt)
    if o.foo == "from base" then acc = acc + 1 end
    acc = acc + #o
  end
  return acc
end
print(meta(1500), meta(9))
-- (the interpreter reads the flags of the metatables the trace built)
for j = 1, 3 do
  local o = setmetatable({}, kept[j])
  print(j, o.foo, #o)
end
-- an integer key between the insertions (its node depends on the key)
local function sorted(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  table.sort(ks)
  return table.concat(ks, ",")
end
local function mixed(n)
  local acc = 0
  local keep = {}
  for i = 1, n do
    local t = {a = 1, b = 2, c = 3, d = 4, e = 5}
    t[i * 3 + 1] = i
    t.f = 6
    t.g = 7
    keep[i % 4 + 1] = t
    acc = acc + t.a + t.f + t.g + t[i * 3 + 1] + (t.zz or 0)
    if i % 50 == 0 then t.h = 8; acc = acc + t.h end
  end
  return acc, keep
end
local ma, mk = mixed(1500)
print(ma, (mixed(11)))
for j = 1, 4 do print(sorted(mk[j])) end
-- an integer key into an empty array slot between the insertions: it lands in the array part or in the hash part, depending on its value
local function mixed2(n)
  local acc = 0
  local keep = {}
  for i = 1, n do
    local t = {nil, nil, nil, a = 1, b = 2, c = 3}
    t[i % 4 + 1] = i
    t.d = 4
    keep[i % 4 + 1] = t
    acc = acc + t.a + t.d + (t[i % 4 + 1] or 0)
  end
  return acc, keep
end
local m2a, m2k = mixed2(1500)
print(m2a, (mixed2(11)))
for j = 1, 4 do print(sorted(m2k[j])) end
-- rehash after the constructor, deletion and re-insertion, a collection in the middle
local function grow(n)
  local acc = 0
  for i = 1, n do
    local t = {p = i, q = 2}
    t.r = 3
    t.p = nil
    t.p = 5
    if i % 100 == 0 then collectgarbage() end
    acc = acc + t.p + t.q + t.r
  end
  return acc
end
print(grow(1500), grow(5))
"##,
    },
    Prog {
        name: "a1_setmt_old",
        small: true,
        src: r##"
-- A1: the inline `setmetatable`: an old (black, in generational mode) table without a metatable gets a young metatable that nothing else
-- references (the collector barrier must not be skipped), plus the cases the inline form hands to the helper: a table that already has a
-- metatable, a metatable that gets `__gc` in the middle of the run, a protected metatable, `nil`.
local function olds(n)
  local t = {}
  for i = 1, n do t[i] = {} end
  return t
end
local function pre(n)
  local mts = {}
  -- (a throw-away use caches "no __gc" in each metatable: only then does the inline form apply)
  for i = 1, n do local m = {__index = {v = i}} setmetatable({}, m) mts[i] = m end
  return mts
end
local function apply(ts, mts, n)
  local junk
  for i = 1, n do
    local mt = mts[i]
    mts[i] = false
    setmetatable(ts[i], mt)
    junk = {i, i, i}   -- (collector steps between the assignments)
  end
  return junk
end
local function check(ts, n)
  local s = 0
  for i = 1, n do s = s + ts[i].v end
  return s
end
local function run(rounds, n)
  local total = 0
  for r = 1, rounds do
    local ts = olds(n)
    collectgarbage() collectgarbage()
    local mts = pre(n)
    apply(ts, mts, n)
    collectgarbage()
    total = total + check(ts, n)
  end
  return total
end
print(run(12, 300))
-- the same table gets metatables one after the other (the second assignment sees an old metatable), `nil` among them
local function again(n)
  local t, s = {}, 0
  local A, B = {__index = {w = 1}}, {__index = {w = 2}}
  for i = 1, n do
    setmetatable(t, i % 3 == 0 and nil or (i % 3 == 1 and A or B))
    s = s + (t.w or 100)
  end
  return s
end
print(again(2000))
-- a finalizer appears on the metatable after the loop is hot, and a metatable becomes protected
local fin = 0
local H = {}
local function mk(n, m)
  for i = 1, n do setmetatable({}, m) end
end
mk(600, H) mk(600, H)
H.__gc = function() fin = fin + 1 end
mk(600, H)
collectgarbage() collectgarbage()
print(fin)
local P = {}
local victims = {}
for i = 1, 40 do victims[i] = setmetatable({}, P) end
P.__metatable = "locked"
local ok = 0
local function relock(n)
  for i = 1, n do
    local v = victims[i % 40 + 1]
    if not pcall(setmetatable, v, H) then ok = ok + 1 end
  end
end
relock(2000)
print(ok)
"##,
    },
    Prog {
        name: "r1_tailcalls",
        small: true,
        src: r##"
-- R1: TAILCALL of tier 1 inline (`tailfast`): the shapes the fast path takes (a Lua closure, a fixed argument count, a compiled
-- callee with enough arguments) and every one it refuses (too few arguments, vararg callee, C function, __call table, caller with
-- captured locals, stack growth), in loops that are hot, with stack growth by deep tail recursion and by a callee with many registers.
local function loop(n, acc) if n == 0 then return acc end return loop(n - 1, acc + n) end
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local function c3(a, b, d) if d == nil then return a + b end return a + b + d end
local function few(x) return c3(x, 1) end            -- one argument short: nil-filled by the callee
local function many(x) return c3(x, 1, 2, 3, 4) end  -- extra arguments are dropped
local function va(...) return select('#', ...) + (... or 0) end
local function tva(x) return va(x, x) end
local function tva2(x) return va() end
local function tc(x) return tostring(x) end
local function tm(x) return math.max(x, 3) end
local callable = setmetatable({}, {__call = function(self, a) return a * 2 end})
local function tcall(x) return callable(x) end
local function captured(x)
  local y = x
  local f = function() return y end   -- y is captured: TAILCALL carries the close flag
  return c3(y, f(), 1)
end
local function big(a)
  local t1, t2, t3, t4, t5, t6, t7, t8, t9, t10 = a, a, a, a, a, a, a, a, a, a
  local u1, u2, u3, u4, u5, u6, u7, u8, u9, u10 = a, a, a, a, a, a, a, a, a, a
  return t1 + t2 + t3 + t4 + t5 + t6 + t7 + t8 + t9 + t10 + u1 + u2 + u3 + u4 + u5 + u6 + u7 + u8 + u9 + u10
end
local function tbig(x) return big(x) end
-- a callee with ~150 registers, tail-called while the stack is still small (the room check sends it to the helper that grows the stack)
local hn, hv = {}, {}
for k = 1, 150 do hn[k] = "v" .. k; hv[k] = "a" end
local huge = load("return function(a) local " .. table.concat(hn, ",") .. " = " .. table.concat(hv, ",") .. " return v1 + v150 + a end")()
local function thuge(x) return huge(x) end
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + loop(12, 0) + few(i) + many(i) + tva(i) + tva2(i) + #tc(i) + tm(i) + tcall(i) + captured(i) + tbig(i) + thuge(i)
    if ev(6 + i % 2) then s = s + 1 end
  end
  return s
end
print(run(300), run(7), run(300))
-- deep tail recursion: the frame is reused, the stack does not grow, the result is right
print(loop(100000, 0))
print(ev(100001), od(100001))
-- fresh coroutines have the smallest stack: the callee with 150 registers needs it to grow (the room check of the inline path)
local function cobody(a) local x = thuge(a) return thuge(x) end
local cs = 0
for k = 1, 40 do cs = cs + coroutine.wrap(cobody)(k) end
print(cs)
-- the callee changes under a hot loop (a different function, then a non-function)
local target = function(a) return a + 1 end
local function tcallee(x) return target(x) end
local function swap(n)
  local s = 0
  for i = 1, n do
    s = s + tcallee(i)
    if i == n // 2 then target = function(a, b) return a * 2 end end
    if i == (3 * n) // 4 then target = setmetatable({}, {__call = function(self, a) return -a end}) end
  end
  return s
end
print(swap(400))
-- tail call to a callee that is compiled late (its first calls are interpreted)
local late = function(a, b, c) return a + b + c end
local function tlate(x) return late(x, 1, 2) end
local acc = 0
for i = 1, 3000 do acc = acc + tlate(i) end
print(acc)
-- traceback and getinfo through tail calls
local function lvl3() return debug.traceback("tb", 1) end
local function lvl2() return lvl3() end
local function lvl1() return lvl2() end
local tb
for i = 1, 400 do tb = lvl1() end
print((tb:gsub("0x%x+", "PTR")))
local function ti() return debug.getinfo(1, "t").istailcall end
local function tti() return ti() end
local r = 0
for i = 1, 400 do if tti() then r = r + 1 end end
print(r)
-- an error raised in the tail callee, caught outside
local function bad(x) if x == 350 then error("boom " .. x) end return x end
local function tbad(x) return bad(x) end
for i = 1, 400 do
  local ok, e = pcall(tbad, i)
  if not ok then print(i, e) end
end
"##,
    },
    Prog {
        name: "r1_tail_co",
        small: true,
        src: r##"
-- R1: tail calls inside coroutines (a yield in the tail callee, in the middle of a hot loop), a hook installed while the loop
-- runs (the fast path refuses with a hook), a count hook that fires inside tail-called functions.
local function y(x) if x % 7 == 0 then coroutine.yield(x) end return x end
local function ty(x) return y(x) end
local function body(n)
  local s = 0
  for i = 1, n do s = s + ty(i) end
  return s
end
local co = coroutine.create(function() return body(500) end)
local yields, last = 0, nil
while true do
  local ok, v = coroutine.resume(co)
  assert(ok, v)
  last = v
  if coroutine.status(co) == "dead" then break end
  yields = yields + 1
end
print(yields, last)
local function leaf(x) return x + 1 end
local function tleaf(x) return leaf(x) end
local cnt = 0
local function work(n)
  local s = 0
  for i = 1, n do
    s = s + tleaf(i)
    if i == n // 3 then debug.sethook(function() cnt = cnt + 1 end, "", 7) end
    if i == (2 * n) // 3 then debug.sethook() end
  end
  return s
end
print(work(900), cnt > 0)
local calls = 0
local function work2(n)
  local s = 0
  for i = 1, n do
    s = s + tleaf(i)
    if i == n // 2 then debug.sethook(function() calls = calls + 1 end, "c") end
  end
  debug.sethook()
  return s
end
print(work2(600), calls > 0)
"##,
    },
    Prog {
        name: "r1_call_loops",
        small: true,
        src: r##"
-- R1: loops with calls in their bodies (C functions, small Lua functions, results used), the shapes whose entry from the
-- interpreter was refused by the back-off monitor and now enters at the sampling tick (`monwarm`): the same results with
-- hooks set mid-run, coroutines, and a loop that exits through the C function.
local floor, max, abs = math.floor, math.max, math.abs
local function f(a, b) return a * 2 + b end
local function g(a) return a // 3 end
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + f(i, 1) + g(i) + max(i, 100) + floor(i / 3) + abs(-i) + #tostring(i)
  end
  return s
end
local total = 0
for r = 1, 40 do total = total + run(500) end
print(total)
local function until_big(n)
  local s, i = 0, 0
  while true do
    i = i + 1
    s = s + max(i, 3)
    if s > n then return i, s end
  end
end
for r = 1, 30 do local a, b = until_big(50000 + r) if r % 10 == 0 then print(a, b) end end
local co = coroutine.wrap(function()
  for i = 1, 3000 do coroutine.yield(max(i, 5) + f(i, 2)) end
  return -1
end)
local cs = 0
for i = 1, 3000 do cs = cs + co() end
print(cs)
local hits = 0
local function hooked(n)
  local s = 0
  for i = 1, n do
    s = s + f(i, 1) + max(i, 7)
    if i == n // 2 then debug.sethook(function() hits = hits + 1 end, "", 11) end
  end
  debug.sethook()
  return s
end
for r = 1, 20 do hooked(2000) end
print(hits > 0)
local obj = {n = 0}
function obj:inc(d) self.n = self.n + d return self.n end
local function method_loop(n)
  for i = 1, n do obj:inc(i % 3) end
  return obj.n
end
for r = 1, 30 do method_loop(1000) end
print(obj.n)
"##,
    },
    Prog {
        name: "m1_interp",
        small: true,
        src: r##"
-- M1: a tree-walking interpreter shape: mutually recursive eval / exec, `RETURN` / `TAILCALL` with the close flag (the functions
-- contain a generic `for` or create closures), nil elements of node arrays, errors / tracebacks / stack overflow inside the
-- recursion, closures that capture a local of a returning frame (an open upvalue at the `RETURN k`), coroutines yielding from
-- inside the recursion, a count hook set mid-run.
local eval, exec
local calls = 0
local function N(k, a, b, c) return { k, a, b, c } end
function eval(e, env)
  calls = calls + 1
  local k = e[1]
  if k == "num" then return e[2]
  elseif k == "var" then return env[e[2]]
  elseif k == "bin" then
    local l, r = eval(e[3], env), eval(e[4], env)
    if e[2] == "+" then return l + r elseif e[2] == "*" then return l * r else return l - r end
  elseif k == "list" then
    local s = 0
    for _, x in ipairs(e[2]) do s = s + eval(x, env) end
    return s
  elseif k == "blk" then return exec(e[2], env)
  elseif k == "bad" then error("bad node " .. tostring(e[2]))
  elseif k == "yield" then return coroutine.yield(eval(e[2], env))
  else return nil end
end
function exec(stmts, env)
  for i = 1, #stmts do
    local s = stmts[i]
    local k = s[1]
    if k == "set" then env[s[2]] = eval(s[3], env)
    elseif k == "if" then
      local r
      if eval(s[2], env) ~= 0 then r = exec(s[3], env) elseif s[4] then r = exec(s[4], env) end
      if r then return r end
    elseif k == "while" then
      while eval(s[2], env) ~= 0 do
        local r = exec(s[3], env)
        if r then return r end
      end
    elseif k == "ret" then return eval(s[2], env)
    elseif k == "tail" then return eval(s[2], env)
    end
  end
end
local prog = {
  N("set", "i", N("num", 0)), N("set", "a", N("num", 0)),
  N("while", N("bin", "-", N("num", 40), N("var", "i")), {
    N("set", "a", N("bin", "+", N("var", "a"), N("bin", "*", N("var", "i"), N("num", 3)))),
    N("if", N("bin", "-", N("var", "i"), N("num", 17)), { N("set", "a", N("bin", "+", N("var", "a"), N("num", 1000))) }),
    N("if", N("bin", "-", N("var", "i"), N("num", 30)), { N("set", "a", N("bin", "-", N("var", "a"), N("num", 5))) },
      { N("set", "a", N("bin", "+", N("var", "a"), N("num", 7))) }),
    N("set", "i", N("bin", "+", N("var", "i"), N("num", 1))),
  }),
  N("ret", N("list", { N("var", "a"), N("var", "i"), N("bin", "*", N("num", 2), N("var", "a")) })),
}
local res
for r = 1, 60 do res = exec(prog, {}) end
print(res, calls)
-- mutual recursion of varying depth around the unroll / depth limits, with a generic for (k set) in both
local isodd
local function iseven(n, tag)
  for _, t in ipairs(tag) do if t == "x" then return false end end
  if n == 0 then return true end
  return isodd(n - 1, tag)
end
function isodd(n, tag)
  for _, t in ipairs(tag) do if t == "x" then return false end end
  if n == 0 then return false end
  return iseven(n - 1, tag)
end
local tag, c = { "a" }, 0
for i = 1, 4000 do
  if iseven(i % 11, tag) then c = c + 1 end
  if isodd(i % 7, tag) then c = c + 2 end
  if i == 3000 then tag[2] = "x" end
end
print(c)
-- closures that capture a local of the returning frame: the RETURN k / TAILCALL k must close the upvalue
local function mk(n, f)
  local cap = n
  local function g() cap = cap + 1; return cap end
  for _, v in ipairs({ 1 }) do cap = cap + v end
  if f then return f(g) end
  return g
end
local fs, sum = {}, 0
for i = 1, 300 do fs[i] = mk(i) end
for i = 1, 300 do sum = sum + fs[i]() + fs[i]() end
local function callit(g) return g() + g() end
for i = 1, 300 do sum = sum + mk(i, callit) end
print(sum)
-- a to-be-closed variable closed by a RETURN k (Lua's OP_TBC registers it in the thread's list, the frame is not marked)
local closed = 0
local function withclose(n)
  local x <close> = setmetatable({}, { __close = function() closed = closed + n end })
  for _, v in ipairs({ 1 }) do n = n + v end
  return n
end
local cs = 0
for i = 1, 300 do cs = cs + withclose(i) end
print(cs, closed)
-- nil array elements in nodes, read in a hot loop
local nodes = {}
for i = 1, 50 do nodes[i] = i % 3 == 0 and { "w", i, 2 } or { "w", i, 2, i % 3 == 1 and "e" or nil } end
local cnt = 0
for r = 1, 60 do
  for i = 1, 50 do local n = nodes[i]; if n[4] then cnt = cnt + 1 elseif n[3] then cnt = cnt + 2 end end
end
print(cnt)
-- errors and tracebacks from inside the recursion
local bad = N("bin", "+", N("num", 1), N("blk", { N("ret", N("bin", "+", N("num", 2), N("bad", 7))) }))
local good = N("bin", "+", N("num", 1), N("blk", { N("ret", N("bin", "+", N("num", 2), N("num", 3))) }))
local nerr, ssum = 0, 0
for i = 1, 400 do
  local ok, m = pcall(eval, i % 40 == 0 and bad or good, {})
  if ok then ssum = ssum + m else nerr = nerr + 1; if nerr == 1 then print(m) end end
end
print(nerr, ssum)
local function tb() return select(2, xpcall(eval, function(m) return (debug.traceback(m, 1):gsub("0x%x+", "0x")) end, bad, {})) end
for i = 1, 3 do local t = tb(); if i == 3 then print(t) end end
-- stack overflow through mutual recursion: the same error text
local function ping(n) return 1 + (n > 0 and ping(n - 1) or 0) end
local function pong(n) local r = ping(n); return r end
if not os.getenv("RUSTYNAIL_JIT_STRESS") then -- (the stack-move stress makes 200000 frames quadratic)
  local ok, m = pcall(function() local function inf(n) return 1 + inf(n + 1) end return inf(1) end)
  print(ok, (tostring(m):gsub("%d+", "N")):sub(1, 60))
else
  print(false, "prog:N: stack overflow")
end
for i = 1, 200 do pong(i % 30) end
-- coroutines yielding from inside the recursion
local co = coroutine.wrap(function()
  local env = {}
  for i = 1, 100 do
    coroutine.yield(eval(N("bin", "+", N("num", i), N("yield", N("num", i * 2))), env))
  end
  return -1
end)
local ys = 0
for i = 1, 200 do ys = ys + (co(1) or 0) end
print(ys)
-- a count hook set in the middle of the run
local hits = 0
for r = 1, 40 do
  if r == 20 then debug.sethook(function() hits = hits + 1 end, "", 97) end
  res = exec(prog, {})
end
debug.sethook()
print(res, hits > 0)
"##,
    },
    Prog {
        name: "m1_gfor_inl",
        small: true,
        src: r##"
-- M1: `ipairs` / `pairs` loops of inlined callees called from hot loops (unrolled by the recorder): lengths 0, 1, 2, 5 and beyond the
-- unroll bound, holes, hash-part continuations, tables that grow or lose elements during the traversal, `break` and `return` inside
-- the loop, nested loops, metatables with `__index` (ipairs honours it), errors inside the body, a table that changes shape mid-run.
local function sum(t)
  local s = 0
  for _, v in ipairs(t) do s = s + v end
  return s
end
local function first_over(t, lim)
  for i, v in ipairs(t) do if v > lim then return i, v end end
  return nil
end
local function count_brk(t)
  local n = 0
  for _, v in ipairs(t) do
    if v == 3 then break end
    n = n + 1
  end
  return n
end
local function nested(a, b)
  local s = 0
  for _, x in ipairs(a) do
    for _, y in ipairs(b) do s = s + x * y end
  end
  return s
end
local function keysum(t)
  local s = 0
  for k, v in pairs(t) do s = s + #k + v end
  return s
end
local function grow(t)
  local n = 0
  for i, v in ipairs(t) do
    n = n + v
    if i == 2 and #t < 6 then t[#t + 1] = 10 end
  end
  return n
end
local function shrink(t)
  local n = 0
  for i, v in ipairs(t) do
    n = n + v
    if i == 1 then t[3] = nil end
  end
  return n
end
local sets = {
  {}, { 1 }, { 1, 2 }, { 1, 2, 3, 4, 5 }, { 1, 2, nil, 4 }, { 7, 7, 7 },
  (function() local t = {} for i = 1, 40 do t[i] = i % 5 end return t end)(),
  (function() local t = { 1, 2 } t[3] = 3 t[4] = 4 return t end)(),
}
local acc = 0
for r = 1, 120 do
  for k = 1, #sets + 1 do
    local t = sets[k] or {}
    acc = acc + sum(t) + count_brk(t)
    local i, v = first_over(t, 2)
    acc = acc + (i or -1) + (v or 0)
  end
  acc = acc + nested(sets[3], sets[4]) + nested(sets[2], sets[7]) + nested({}, sets[4])
  acc = acc + keysum({ a = 1, bb = 2, ccc = r % 4 })
  local g = { 1, 2 }
  acc = acc + grow(g) + #g
  local h = { 5, 6, 7, 8 }
  acc = acc + shrink(h)
end
print(acc)
-- ipairs through __index (5.5 ipairs respects it), and a shape change in the middle of the run
local proxy = setmetatable({}, { __index = function(_, i) if i <= 3 then return i * 10 end end })
local pacc = 0
for r = 1, 200 do pacc = pacc + sum(proxy) end
print(pacc)
local shape = { 1, 2, 3 }
local sacc = 0
for r = 1, 400 do
  if r == 200 then shape = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20 } end
  if r == 300 then shape = { "a" } end
  local ok, e = pcall(sum, shape)
  sacc = sacc + (ok and e or #e)
end
print(sacc)
-- a loop whose body errors on one element, and a coroutine that yields inside the loop of an inlined callee
local function bad(t) for _, v in ipairs(t) do if v == 4 then error("four") end end return 0 end
local ne = 0
for r = 1, 100 do
  local ok, m = pcall(bad, r % 10 == 0 and { 1, 4 } or { 1, 2 })
  if not ok then ne = ne + #m end
end
print(ne)
local function yielder(t) local s = 0 for _, v in ipairs(t) do s = s + coroutine.yield(v) end return s end
local co = coroutine.wrap(function() for r = 1, 50 do yielder({ 1, 2, 3 }) end return -1 end)
local ys = 0
for r = 1, 150 do ys = ys + (co(1) or 0) end
print(ys)
"##,
    },
    Prog {
        name: "m1_rd",
        small: true,
        src: r##"
-- M1: a recursive-descent evaluator (expr -> term -> factor -> expr) called from a hot loop: inlining depth beyond the limit
-- (stitching exit), `while` loops in the callees (unrolled), token arrays that end (nil past the array), parenthesis depth that
-- varies from run to run, an error from the depth, and a function that changes in the middle of the run.
local toks, pos
local expr
local function factor()
  local t = toks[pos]
  pos = pos + 1
  if t == "(" then
    local v = expr()
    pos = pos + 1
    return v
  end
  if t == "-" then return -factor() end
  if type(t) ~= "number" then error("bad token " .. tostring(t)) end
  return t
end
local function term()
  local v = factor()
  while toks[pos] == "*" do pos = pos + 1; v = v * factor() end
  return v
end
function expr()
  local v = term()
  while toks[pos] == "+" or toks[pos] == "-" do
    local op = toks[pos]
    pos = pos + 1
    if op == "+" then v = v + term() else v = v - term() end
  end
  return v
end
local progs = {
  { 3, "+", 4, "*", "(", 2, "+", 5, ")", "-", 6, "*", 2, "+", 7, "*", 3 },
  { 1 },
  { "(", "(", "(", "(", "(", 1, "+", 2, ")", "*", 3, ")", "-", 4, ")", "+", 5, ")", "*", 2, ")" },
  { 2, "*", "-", 3, "+", 4 },
  { 5, "*", "*", 3 },
  { "(", 1, "+" },
}
local s, errs = 0, 0
for r = 1, 500 do
  toks = progs[r % 3 + 1]
  pos = 1
  s = s + expr()
end
print(s)
for r = 1, 600 do
  toks = progs[r % #progs + 1]
  pos = 1
  local ok, v = pcall(expr)
  if ok then s = s + v else errs = errs + 1; if errs == 1 then print(v) end end
  if r == 400 then progs[1] = { 3, "+", 4, "+", 5, "*", 6 } end
end
print(s, errs)
-- the same through a coroutine, and with a hook set in the middle
local co = coroutine.wrap(function()
  for r = 1, 200 do
    toks = progs[r % 3 + 1]
    pos = 1
    coroutine.yield(expr())
  end
  return -1
end)
local cs = 0
for r = 1, 150 do cs = cs + (co() or 0) end
print(cs)
local hits = 0
for r = 1, 300 do
  if r == 100 then debug.sethook(function() hits = hits + 1 end, "", 23) end
  toks = progs[r % 3 + 1]
  pos = 1
  s = s + expr()
end
debug.sethook()
print(s, hits > 0)
"##,
    },
    Prog {
        name: "m1_deep",
        small: true,
        src: r##"
-- M1: a call chain deeper than the inlining depth called from a hot loop (the recording ends with a stitching exit before the
-- call beyond the limit), results that depend on the iteration, an error from the bottom, and a callee that changes mid-run.
local f0, f1, f2, f3, f4, f5, f6
function f1(x) return f2(x + 1) + 1 end
function f4(x) if x % 7 == 0 then local r = f5(x) return r + 1 end local r = f5(x) return r * 2 end
function f5(x) local r = f6(x) return r - 3 end
function f6(x) if x == 4000 then error("bottom " .. x) end return x % 11 end
function f3(x) local r = f4(x) return r + 1 end
function f2(x) local r = f3(x) return r + 2 end
function f0(x) local r = f1(x) return r end
local s, e = 0, 0
for i = 1, 3000 do
  local t = i * 2 + 1
  t = t * 3 - i
  t = t + (i % 5) * 2 + (i % 3) * 7
  t = t - (i % 11) * 4 + (i % 13)
  t = (t + i // 3) % 1000
  t = t + (t % 7) * 3 + (t % 9)
  s = s + t % 17 + f0(i)
  if i == 1500 then f6 = function(x) return x % 5 end end
end
print(s)
for i = 3990, 4010 do
  local ok, m = pcall(f0, i)
  if ok then s = s + m else e = e + 1; print(m) end
end
print(s, e)
"##,
    },
    Prog {
        name: "f1_folds",
        small: true,
        src: r##"
-- F1: constant-exponent pow folds (x^2 is x*x, other exponents stay helper calls), math.sin/cos/exp/log and n-ary math.min/max as pure helper
-- calls / typed IR, string.len / s:len() / #s as header loads (with the string metatable and string.len changed mid-run), against the
-- interpreter in every mode.
local function run(name, f, n) local r = f(n); print(name, r) end
local s = "hello world"
local big = string.rep("x", 100)
local function sq(n) local a = 0.0 for i = 1, n do local x = i * 0.5 - 3; a = a + x ^ 2 + (i % 3) ^ 2 + (-x) ^ 2.0 end return a end
run("pow2", sq, 300)
local function sqnan(n) local a, b = 0.0, 1/0 for i = 1, n do a = a + (-b) ^ 2 + (0/0 == 0/0 and 1 or 0) + ((-0.0) ^ 2) end return a end
run("pow2inf", sqnan, 300)
local function pw(n) local a = 0.0 for i = 1, n do a = a + i ^ 3 + i ^ 0.5 + 2 ^ i end return a end
run("pow3", pw, 100)
local function trig(n) local a = 0.0 for i = 1, n do a = a + math.sin(i) + math.cos(i * 0.5) + math.exp(i / 100) + math.log(i) end return a end
run("trig", trig, 400)
local function trigspec(n) local a = {} for i = 1, n do local x = (i - 5) * 1e300; a[#a + 1] = tostring(math.sin(x)) .. tostring(math.cos(-x)) .. tostring(math.log(i - 5)) .. tostring(math.exp(x)) .. tostring(math.log(0)) end return table.concat(a, ",") end
print(trigspec(10))
local function mm(n) local a = 0 for i = 1, n do a = a + math.max(i, 100, 7) + math.min(i, 50, 3, 99) + math.max(1, 2, 3, i % 5) end return a end
run("mm3", mm, 300)
local function mmf(n) local a = 0.0 for i = 1, n do local x = i * 0.5 a = a + math.max(x, 100.5, 7.0) + math.min(x, 50.25, 3.5, 99.0) + math.max(0/0, x, 1.0) end return a end
run("mm3f", mmf, 300)
local function mmnan(n) local a = {} for i = 1, n do a[#a+1] = tostring(math.max(0/0, i + 0.0, 1.0)) .. tostring(math.min(i + 0.0, 0/0, 1.0)) .. tostring(math.max(-0.0, 0.0, -0.0)) end return table.concat(a, ",") end
print(mmnan(5))
local function mmmix(n) local a = 0 for i = 1, n do a = a + math.max(i, 2.5, 3) end return a end
run("mmmix", mmmix, 100)
local function slen(n) local a = 0 for i = 1, n do a = a + #s + s:len() + string.len(s) + string.len(big) + big:len() end return a end
run("slen", slen, 300)
local function slen2(n) local a = 0 for i = 1, n do local t = (i % 2 == 0) and s or big; a = a + t:len() + string.len(t) end return a end
run("slen2", slen2, 300)
local function slenerr(n) local a = 0 for i = 1, n do local ok, e = pcall(string.len); local ok2, e2 = pcall(string.len, {}); a = a + (ok and 1 or 0) + (ok2 and 1 or 0) + string.len(i) + string.len(i + 0.5) end return a end
run("slen_num", slenerr, 100)
local function slen3(n) local a = 0 for i = 1, n do a = a + string.len(s, "extra") end return a end
run("slen_extra", slen3, 100)
local function slen4(n) local a = 0 for i = 1, n do if i == 150 then string.len = function(x) return 7 end end a = a + string.len(s) end return a end
run("slen_replaced", slen4, 300)
local function mtswap(n) local a = 0 for i = 1, n do if i == 150 then getmetatable("").__index = { len = function() return 5 end } end a = a + s:len() end return a end
run("mt_swapped", mtswap, 300)
local function fl(n)
  local a, out = 0, {}
  for i = 1, n do
    local x = (i - 40) * 0.37
    a = a + math.floor(x) + math.ceil(x) + math.floor(x * 1e3) + math.ceil(-x * 1e3) + math.floor(i) + math.ceil(i)
    a = a + (x // 1.0) + (x // 0.3) + (-x // 2.5) + (i // 3) + (i // 2.0)
  end
  return a
end
run("floor", fl, 300)
local function flsp(n)
  local out = {}
  local vals = { 0.0, -0.0, 0.5, -0.5, 1e15 + 0.5, -1e15 - 0.5, 2^52, -(2^52) - 1, 2^62, 1e300, 1/0, -1/0, 0/0, 3.0, -3.0, 0.9999999999999999 }
  for i = 1, n do
    local x = vals[i % #vals + 1]
    out[#out + 1] = tostring(x // 1.0) .. "," .. tostring(x // -1.0) .. "," .. tostring(-x // 0.3)
    local ok, f = pcall(math.floor, x); local ok2, c = pcall(math.ceil, x)
    out[#out + 1] = tostring(ok and f) .. tostring(ok2 and c) .. tostring(math.type(f))
  end
  return table.concat(out, ";")
end
print(flsp(200))
"##,
    },
    Prog {
        name: "f1_next_hint",
        small: true,
        src: r##"
-- F1: `next` / `pairs` with the carried traversal index: order, assigning nil to existing fields during traversal, new keys of other
-- tables between steps (a stale hint), nested traversals, rehash between steps, a table that changed identity, float / boolean / table
-- / string keys, and the errors of `next`.
local function keys(t) local r = {} for k, v in pairs(t) do r[#r + 1] = tostring(k) .. "=" .. tostring(v) end return table.concat(r, ",") end
local h = {}
for i = 1, 40 do h["k" .. i] = i end
for i = 1, 10 do h[i * 1.5] = i end
h[true] = "t"; h[false] = "f"
local function trav(n)
  local s = 0
  for r = 1, n do
    for k, v in pairs(h) do if type(v) == "number" then s = s + v end end
  end
  return s
end
print(trav(50), #keys(h))
-- assigning nil to existing fields during the traversal
local d = {}
for i = 1, 30 do d["a" .. i] = i; d[i + 100] = i end
local cnt, sum = 0, 0
for k, v in pairs(d) do d[k] = nil; cnt = cnt + 1; sum = sum + v end
print(cnt, sum, next(d))
-- updating existing fields during the traversal
local u = {}
for i = 1, 20 do u["u" .. i] = i end
for k, v in pairs(u) do u[k] = v * 2 end
local tot = 0
for k, v in pairs(u) do tot = tot + v end
print(tot)
-- two interleaved traversals of two tables (the hint belongs to the last one)
local A, B = {}, {}
for i = 1, 25 do A["x" .. i] = i; B["y" .. i] = -i end
local ka, kb, n, sa, sb = nil, nil, 0, 0, 0
while true do
  local va, vb
  ka, va = next(A, ka)
  kb, vb = next(B, kb)
  if ka == nil and kb == nil then break end
  n = n + 1; sa = sa + (va or 0); sb = sb + (vb or 0)
end
print(n, sa, sb)
-- nested traversals of the same and of different tables
local c = 0
for k1 in pairs(A) do for k2 in pairs(A) do c = c + 1 end for k3 in pairs(B) do c = c + 1 end end
print(c)
-- a traversal that is interrupted by a rehash of the table (inserting new keys is undefined; only existing-key assignment is not)
local r = {}
for i = 1, 8 do r["r" .. i] = i end
local before = keys(r)
for i = 1, 64 do r["n" .. i] = i end
for i = 1, 64 do r["n" .. i] = nil end
print(before == keys(r) or "order differs", next({}), next({}, nil))
-- errors
print(pcall(next, {}, "nokey"))
print(pcall(next, {10, 20}, 3))
print(pcall(next, {10, 20}, 2))
print(pcall(next, h, {}))
print(select("#", next({})))
-- keys of every type, array + hash, integer keys beyond the array, negative and zero keys
local m = {10, 20, 30, [0] = "z", [-1] = "n", [1000] = "big", [2^53] = "f", [1.5] = "h", [true] = 1}
print(keys(m))
local last
for k in pairs(m) do last = k end
print(next(m, last))
-- next with a key that was removed (assigned nil) and then asked for
local rm = {}
for i = 1, 10 do rm["q" .. i] = i end
local firstk = next(rm)
rm[firstk] = nil
print((pcall(next, rm, firstk)))
collectgarbage()
print((pcall(next, rm, firstk)))
-- a loop long enough to be compiled, with a table switch halfway
local P, Q = {}, {}
for i = 1, 50 do P["p" .. i] = i; Q["q" .. i] = i * 2 end
local function walk(n)
  local s = 0
  for r = 1, n do
    local t = (r % 3 == 0) and Q or P
    for k, v in pairs(t) do s = s + v end
    for k, v in next, t do s = s + v end
  end
  return s
end
print(walk(200))
local function walk2(n)
  local s = 0
  local t = {}
  for i = 1, 20 do t["w" .. i] = i end
  for r = 1, n do
    for k, v in pairs(t) do
      if r % 5 == 0 then t[k] = v + 1 end
      s = s + v
    end
    if r == n // 2 then for i = 1, 20, 2 do t["w" .. i] = nil end end
  end
  return s
end
print(walk2(300))
"##,
    },
    Prog {
        name: "f2_byte",
        small: true,
        src: r##"
-- F2: string.byte with the inline fast path and the recorded result ranges
local N = 400
local function rep40(s) return s .. string.rep("x", 40) end
-- string.byte: one position (inline for 1 <= i <= #s), from the end, past the end, zero, long strings, ranges (the count must hold)
local BS = { "abcdefghij", rep40("q") .. "rst", string.rep("0123456789", 20), "", "x" }
local bsum, bcnt = 0, 0
for r = 1, N do
  for k = 1, #BS do
    local s = BS[k]
    for i = 1, 14 do
      local b = string.byte(s, i)
      if b then bsum = bsum + b else bcnt = bcnt + 1 end
      local e = s:byte(-i)
      if e then bsum = bsum + e * 2 else bcnt = bcnt + 3 end
    end
    local z = string.byte(s, 0)
    if z == nil then bcnt = bcnt + 1 end
  end
end
print(bsum, bcnt)
local rsum, rn = 0, 0
for r = 1, N do
  for k = 1, #BS - 1 do
    local s = BS[k]
    for i = 1, 6 do
      local a, b, c = string.byte(s, i, i + 2)
      rsum = rsum + (a or 0) + (b or 0) * 3 + (c or 0) * 5
      local x, y = string.byte(s, i, i + 1)
      if y then rn = rn + 1 end
      local t = { string.byte(s, i, i + 3) }
      rn = rn + #t
    end
  end
end
print(rsum, rn)
local JS = { 2, 2, 2, 2, 2, 2, 2, 2, 3, 2, 2, 4, 2, 2, 2, 2 }
local IS = { 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, -3, 2, 0, 2, 2 }
local vsum, vn = 0, 0
for r = 1, N * 6 do
  for k = 1, #JS do
    local a, b, e = string.byte(BS[1], 1, JS[k])
    vsum = vsum + (a or 0) + (b or 0) + (e or 0) * 13
    vn = vn + 1
    local c, d = string.byte(BS[3], IS[k], IS[k] + 1)
    vsum = vsum + (c or 0) * 7 + (d or 0) * 11
  end
end
print(vsum, vn)
"##,
    },
    Prog {
        name: "c2_tier1",
        small: true,
        src: r##"
-- C2: comparisons of non-numbers and SETUPVAL of collectable values (tier-1 helpers `jit_cmp` / `jit_setupval`) in loops that traces cover:
-- strings, tables with and without __eq, functions, booleans, kinds that change mid-loop, error comparisons, upvalues holding fresh objects.
local N = 300
local eqmt = { __eq = function(a, b) return a.v == b.v end }
local plain = { __index = function(t, k) return k end }
local vals = {}
for i = 1, 36 do
  local k = i % 6
  if k == 0 then vals[i] = "s" .. (i % 5)
  elseif k == 1 then vals[i] = string.rep("l", 41) .. (i % 3)
  elseif k == 2 then vals[i] = setmetatable({ v = i % 4 }, eqmt)
  elseif k == 3 then vals[i] = setmetatable({ v = i % 4 }, plain)
  elseif k == 4 then vals[i] = (i % 2 == 0) and print or tostring
  else vals[i] = i * 0.5 end
end
local S = {}
for i = 1, 40 do S[i] = "key" .. (i * 7 % 40) end
local up, upn = {}, 0
local function setup(v) up = v; upn = upn + 1 end
local acc, lt, le, eq = 0, 0, 0, 0
for r = 1, N do
  for i = 1, 40 do
    local a, b = S[i], S[i % 40 + 1]
    if a < b then lt = lt + 1 end
    if a <= b then le = le + 1 end
    if a == b then eq = eq + 1 end
    if S[i] ~= S[(i + r) % 40 + 1] then acc = acc + 1 end
    setup(a)
    if i % 9 == 0 then setup({ r, i }) end
  end
  local v1, v2 = vals[r % 36 + 1], vals[(r * 5) % 36 + 1]
  if v1 == v2 then acc = acc + 3 end
  local ok = pcall(function() return v1 < v2 end)
  if not ok then acc = acc + 5 end
  if r == N // 3 then eqmt.__eq = nil end
  if r == N // 2 then eqmt.__eq = function() return true end end
  if r == (2 * N) // 3 then vals[2] = "s0"; vals[3] = vals[2] end
  if r % 50 == 0 then collectgarbage("step", 0) end
end
print(acc, lt, le, eq, upn, type(up))
"##,
    },
    Prog {
        name: "bb_cmpmm",
        small: true,
        src: r##"
-- BUNDLE-B: `<` `<=` `==` on tables with Lua-function `__lt` / `__le` / `__eq` called from tier 1 (the metamethod frame runs in place, its
-- RETURN finishes the comparison in compiled code): results of every type, a metamethod on one operand, errors and yields inside it, a
-- metamethod replaced mid-run, mixed operand types, sort comparators.
local N = 300
local calls = 0
local mt = {}
mt.__lt = function(a, b) calls = calls + 1; return a.v < b.v end
mt.__le = function(a, b) calls = calls + 1; return a.v <= b.v end
mt.__eq = function(a, b) calls = calls + 1; return a.v == b.v end
local function new(v) return setmetatable({ v = v }, mt) end
local function lt(a, b) return a < b end
local function le(a, b) return a <= b end
local function eq(a, b) return a == b end
local function ngt(a, b) if not (a > b) then return 1 end return 2 end
local x, y, z = new(1), new(2), new(2)
local objs = { x, y, z, new(-1), {}, 5, "s", new(7) }
local acc, bad = 0, 0
for i = 1, N do
  local a, b = objs[i % 8 + 1], objs[(i * 3) % 8 + 1]
  local ok, r = pcall(lt, a, b)
  if ok then acc = acc + (r and 1 or 0) else bad = bad + 1 end
  ok, r = pcall(le, a, b)
  if ok then acc = acc + (r and 2 or 0) else bad = bad + 1 end
  ok, r = pcall(eq, a, b)
  if ok then acc = acc + (r and 4 or 0) else bad = bad + 1 end
  ok, r = pcall(ngt, b, a)
  if ok then acc = acc + r * 8 else bad = bad + 1 end
  if i == N // 2 then mt.__lt = function(p, q) return nil end end
  if i == (2 * N) // 3 then mt.__lt = nil end
  if i == (5 * N) // 6 then mt.__lt = function(p, q) return p.v > q.v end end
end
print(acc, bad, calls)
-- direct (no pcall) loop, results of every type
local rv = { true, 0, nil }
local rmt = { __lt = function() return rv[1] end, __le = function() return rv[2] end, __eq = function() return rv[3] end }
local r1, r2 = setmetatable({}, rmt), setmetatable({}, rmt)
local n = 0
for i = 1, N do
  if r1 < r2 then n = n + 1 end
  if r1 <= r2 then n = n + 10 end
  if r1 == r2 then n = n + 100 end
  if i == N // 2 then rv[1], rv[2], rv[3] = false, nil, "x" end
end
print(n)
-- yields inside the metamethod
local ymt = { __lt = function(a, b) return coroutine.yield("lt") end, __eq = function(a, b) return coroutine.yield("eq") end }
local y1, y2 = setmetatable({}, ymt), setmetatable({}, ymt)
local co = coroutine.wrap(function()
  local c = 0
  for i = 1, N // 3 do
    if y1 < y2 then c = c + 1 end
    if y1 == y2 then c = c + 10 end
  end
  return "done", c
end)
local steps, tag, res = 0, co(), nil
while tag ~= "done" do steps = steps + 1; tag, res = co(steps % 3 ~= 0) end
print(steps, res)
-- errors inside the metamethod and sort comparators
local emt = { __lt = function(a, b) error("lt " .. a.v) end, __eq = function(a, b) return nil + 1 end }
local e1, e2 = setmetatable({ v = 1 }, emt), setmetatable({ v = 2 }, emt)
local errs = 0
for i = 1, N do
  if not pcall(lt, e1, e2) then errs = errs + 1 end
  if not pcall(eq, e1, e2) then errs = errs + 2 end
end
local lst = {}
for i = 1, 60 do lst[i] = new((i * 7) % 23) end
table.sort(lst)
table.sort(lst, function(a, b) return b < a end)
print(errs, select(2, pcall(lt, e1, e2)):gsub("^.-:%d+: ", ""), lst[1].v, lst[60].v)
-- the C-stack count must come back after each metamethod comparison (200 would overflow if it leaked)
local k = 0
for i = 1, 3 * N do if x < y then k = k + 1 end end
print(k)
"##,
    },
    Prog {
        name: "t3_index",
        small: true,
        src: r##"
-- T3: the inline first `__index` level of the tier-1 GETFIELD / SELF / GETTABUP templates: every shape of receiver miss in hot loops, with
-- the situation changed in the middle of the loop (metatable swapped, `__index` table -> function -> nil -> table, rawset shadowing,
-- a dead key in a class that hides a deeper one, a field of the metatable that is not in the `__index` table, GC steps).
local N = 240
local out = {}
local function emit(...) out[#out + 1] = table.concat({ ... }, ",") end
-- 1. receiver hit / class hit / 2- and 3-level chains / nil key / SELF
local A = { a = 1, who = function() return "A" end }
A.__index = A
local B = setmetatable({ b = 2, who = function() return "B" end }, A)
B.__index = B
local C = setmetatable({ c = 3 }, B)
C.__index = C
local o1 = setmetatable({ own = 10 }, A)
local o2 = setmetatable({ own = 20 }, B)
local o3 = setmetatable({ own = 30 }, C)
local s1, s2, s3, nils, ms = 0, 0, 0, 0, 0
for i = 1, N do
  s1 = s1 + o1.own + o1.a
  s2 = s2 + o2.own + o2.a + o2.b
  s3 = s3 + o3.own + o3.a + o3.b + o3.c
  if o3.zzz == nil and o1.nothing == nil then nils = nils + 1 end
  if o1:who() == "A" then ms = ms + 1 end
  if o2:who() == "B" then ms = ms + 2 end
  if o3:who() == "B" then ms = ms + 4 end
end
emit(s1, s2, s3, nils, ms)
-- 2. a dead key (set to nil) in the class hides nothing: the deeper level answers; the receiver's own dead key falls through to the class
local P = { v = "P" }
P.__index = P
local Q = setmetatable({ v = "Q" }, P)
Q.__index = Q
local q = setmetatable({ v = "own" }, Q)
local acc = {}
for i = 1, N do
  if i == 60 then Q.v = nil end        -- dead key in the middle class: P.v answers
  if i == 120 then q.v = nil end       -- dead key in the receiver
  if i == 180 then Q.v = "Q2" end
  acc[#acc + 1] = q.v
end
emit(#acc, acc[1], acc[70], acc[130], acc[190], acc[N])
-- 3. the metatable swapped between calls / __index changed table -> function -> nil -> table / rawset shadows the class
local X = { f = "X" }
local Y = { f = "Y" }
local obj = setmetatable({}, { __index = X })
local cnt = { X = 0, Y = 0, F = 0, N = 0, R = 0 }
for i = 1, N do
  local mt = getmetatable(obj)
  if i == 40 then setmetatable(obj, { __index = Y }) end
  if i == 80 then getmetatable(obj).__index = function(_, k) return "F" end end
  if i == 120 then getmetatable(obj).__index = nil end
  if i == 140 then getmetatable(obj).__index = X end
  if i == 170 then rawset(obj, "f", "R") end
  if i == 200 then obj.f = nil; rawset(obj, "f", nil) end
  if i == 220 then setmetatable(obj, nil) end
  local v = obj.f
  if v == nil then cnt.N = cnt.N + 1 else cnt[v] = (cnt[v] or 0) + 1 end
end
emit(cnt.X, cnt.Y, cnt.F, cnt.N, cnt.R)
-- 4. a field of the metatable that is not in its `__index` table is not an inherited field; `__index` itself is an ordinary name
local mtonly = { inmt = "bad" }
mtonly.__index = { inidx = "good" }
local m = setmetatable({}, mtonly)
local r = {}
for i = 1, N do
  r[1] = m.inmt; r[2] = m.inidx; r[3] = m.__index and "idx"
end
emit(tostring(r[1]), tostring(r[2]), tostring(r[3]))
-- 5. non-table `__index` values: string, number, boolean, userdata-free function, nested function chain
local kinds = { "str", 7, true, function(t, k) return k .. "!" end, setmetatable({}, { __index = function(t, k) return "deep:" .. k end }), false }
local res = {}
for i = 1, N do
  local k = kinds[i % #kinds + 1]
  local ok, v = pcall(function() local z = setmetatable({}, { __index = k }); return z.len end)
  res[#res + 1] = tostring(ok) .. ":" .. (type(v) == "function" and "fn" or tostring(v):gsub("^.-:%d+: ", ""))
end
emit(res[1], res[2], res[3], res[4], res[5], res[6])
-- 6. GETTABUP through an `_ENV` whose metatable has an `__index` table; a global that appears / disappears
local base = { gx = 5, gy = 6 }
local env = setmetatable({}, { __index = base })
local f = load("local s = 0 for i = 1, 200 do s = s + gx + gy + (gz or 0) end return s", "t3env", "t", env)
local g = load("local s = 0 for i = 1, 120 do s = s + gx; if i == 60 then gx = 100 end end return s", "t3env2", "t", env)
local h = load("local s = 0 for i = 1, 120 do s = s + (gy or 1000); if i == 40 then base.gy = nil end; if i == 80 then base.gy = 9 end end return s", "t3env3", "t", setmetatable({ base = base }, { __index = base }))
emit(f(), g(), h())
env.gz = 1000
emit(f())
-- 7. errors: indexing nil / number through a miss keeps the variable names; loop in gettable
local c1, c2 = {}, {}
setmetatable(c1, { __index = c2 }); setmetatable(c2, { __index = c1 })
local errs = {}
for i = 1, 40 do
  local ok, e = pcall(function() return c1.nothing end)
  if i == 40 then errs[#errs + 1] = tostring(e):gsub("^.-:%d+: ", "") end
  local up = nil
  local ok2, e2 = pcall(function() return up.field end)
  if i == 40 then errs[#errs + 1] = tostring(e2):gsub("^.-:%d+: ", "") end
  local ok3, e3 = pcall(function() local num = 5; return num:nomethod() end)
  if i == 40 then errs[#errs + 1] = tostring(e3):gsub("^.-:%d+: ", "") end
  local ok4, e4 = pcall(function() return o1:nomethod() end)
  if i == 40 then errs[#errs + 1] = tostring(e4):gsub("^.-:%d+: ", "") end
end
emit(table.concat(errs, "|"))
-- 8. method lookup through a class whose methods are replaced / removed while the loop runs; strings (`s:upper()`) and tables mixed
local K = {}
K.__index = K
function K:get() return self.v end
local ks = {}
for i = 1, 8 do ks[i] = setmetatable({ v = i }, K) end
local tot = 0
for i = 1, N do
  if i == 80 then function K:get() return self.v * 2 end end
  if i == 160 then K.get = nil end
  local o = ks[i % 8 + 1]
  local okk, v = pcall(function() return o:get() end)
  tot = tot + (okk and v or 1000)
  tot = tot + #("x"):upper()
  if i % 50 == 0 then collectgarbage("step", 0) end
end
emit(tot)
-- 9. instances created and dropped (GC) with the class reached through the first level
local Cls = {}
Cls.__index = Cls
function Cls.new(x) return setmetatable({ x = x }, Cls) end
function Cls:get() return self.x end
local gs = 0
for i = 1, N * 3 do
  local o = Cls.new(i)
  gs = gs + o:get()
  if i % 97 == 0 then collectgarbage() end
end
emit(gs)
for _, l in ipairs(out) do print(l) end
"##,
    },
    Prog {
        name: "f2_lib",
        small: true,
        src: r##"
-- F2: library calls recorded as trace constructs: string comparisons (long `==`, `<`, `<=`, `>`, `>=`), getmetatable, rawget / rawset,
-- assert. Every guard has a path where it fails mid-loop (outcome flips, metatable appears or changes, protected metatable, falsy value).
local N = 400
local function rep40(s) return s .. string.rep("x", 40) end
-- 1. string comparisons
local A, B = {}, {}
for i = 1, 64 do
  A[i] = rep40("k" .. (i % 7)) .. "tail"
  B[i] = rep40("k" .. ((i + (i > 40 and 1 or 0)) % 7)) .. "tail"   -- equal content, different objects, for the first 40
end
A[10] = ""; B[10] = ""; A[11] = "a\0b"; B[11] = "a\0c"; A[12] = "abc"; B[12] = "abcd"
local eq, lt, le, gt, ge, mix = 0, 0, 0, 0, 0, 0
for r = 1, N do
  for i = 1, 64 do
    local a, b = A[i], B[i]
    if a == b then eq = eq + 1 end
    if a < b then lt = lt + 1 end
    if a <= b then le = le + 1 end
    if a > b then gt = gt + 1 end
    if a >= b then ge = ge + 1 end
    if a ~= b then mix = mix + 1 end
    if r == N // 2 and i == 3 then B[i + 20] = A[i + 20] end   -- flips an outcome halfway
  end
end
print(eq, lt, le, gt, ge, mix)
-- short against long, short against short (interned: pointer compare, order by content)
local S = { "a", "b", "ab", "", "abc", rep40("a"), rep40("b") }
local cnt = 0
for r = 1, N do
  for i = 1, #S do
    for j = 1, #S, 2 do
      if S[i] < S[j] then cnt = cnt + 1 end
      if S[i] <= S[j] then cnt = cnt + 2 end
      if S[i] == S[j] then cnt = cnt + 3 end
    end
  end
end
print(cnt)
-- an order comparison of a string with a number is an error: the same message from the trace
local ok, e = pcall(function() local t = 0; for i = 1, 100 do if i == 90 then t = t + (A[1] < 1 and 1 or 0) end end end)
print(ok, e)
-- 2. getmetatable
local mt1, mt2 = { __index = function() return 1 end }, {}
local objs = {}
for i = 1, 50 do objs[i] = setmetatable({}, (i % 5 == 0) and mt2 or mt1) end
objs[7] = {}
local g1, g2, g3 = 0, 0, 0
for r = 1, N do
  for i = 1, 50 do
    local m = getmetatable(objs[i])
    if m == mt1 then g1 = g1 + 1 elseif m == mt2 then g2 = g2 + 1 else g3 = g3 + 1 end
    if r == N // 3 and i == 20 then setmetatable(objs[i], mt1) end
    if r == N // 2 and i == 1 then mt2.__metatable = "locked" end    -- protected from now on: the result is the field
    if r == 3 * N // 4 and i == 1 then mt2.__metatable = nil end
    if r == 9 * N // 10 and i == 1 then mt1.__metatable = false end   -- false is a value too
  end
end
print(g1, g2, g3, getmetatable(objs[5]) == mt2, getmetatable(objs[1]) == mt1)
local gs = 0
for i = 1, 200 do
  local x = (i % 3 == 0) and "s" or ((i % 3 == 1) and objs[i % 50 + 1] or nil)
  if x ~= nil then gs = gs + (getmetatable(x) and 1 or 0) end
end
print(gs, select("#", getmetatable({})), getmetatable(setmetatable({}, { __metatable = 5 })), getmetatable({}))
print(pcall(getmetatable))
-- 3. rawget / rawset
local T = {}
for i = 1, 16 do T[i] = i end
local log = {}
setmetatable(T, { __index = function(t, k) log[#log + 1] = k; return -k end, __newindex = function(t, k, v) log[#log + 1] = -k; rawset(t, k, v) end })
local rs = 0
for r = 1, N do
  for k = 1, 20 do
    local v = rawget(T, k)
    if v then rs = rs + v end
    rawset(T, (k % 16) + 1, r + k)
    if r == N // 2 and k == 5 then T[3] = nil end
    if r == (2 * N) // 3 and k == 5 then rawset(T, 21, "grow") end
  end
end
print(rs, #log, rawget(T, 3), rawget(T, 21), rawlen(T))
local strs = {}
for i = 1, 30 do strs[i] = "a" end
local keep = {}
for r = 1, 200 do
  for i = 1, 30 do rawset(strs, i, { r, i }); keep[i] = rawget(strs, i) end
end
print(#keep, keep[30][1], rawset(strs, 1, 5) == strs)
print(pcall(rawget, 1, 2))
print(pcall(rawset, {}, nil, 1))
print(select("#", rawset(T, 1, 1)), select("#", rawget(T, 1)))
-- 5. math.random: integer forms draw from the interpreter's generator state (-1 and empty intervals go to the interpreter)
math.randomseed(12345)
local rs1, rs2, rs3, rs4 = 0, 0, 0, 0
for r = 1, N do
  for i = 1, 20 do
    rs1 = rs1 + math.random(1, 100)
    rs2 = rs2 + math.random(i + 5)
    rs3 = rs3 + math.random(-3, 3 + i)
    rs4 = rs4 + math.random(i, i)
  end
end
for i = 1, 300 do rs4 = rs4 + math.random(-1, -1) end   -- a draw of -1 is the interpreter's
print(rs1, rs2, rs3, rs4, math.random(1000))
print(pcall(function() for i = 1, 100 do math.random(5 - i // 10) end end))
print(pcall(function() for i = 1, 100 do math.random(i, 60) end end))
-- 4. assert
local function pass(n)
  local s = 0
  for i = 1, n do
    local v = assert(i % 7 == 0 and i or i + 1, "never")
    local a, b, c = assert(i, "m", 3)
    local t = { assert(i, i + 1) }
    s = s + v + a + #b + c + #t
    assert(true)
    assert(s)
    assert("str")
    assert(1.5)
    local x = assert({})
    local y = assert(i < 1000 or nil, "i")
    s = s + (y and 1 or 0)
    select("#", assert(1, 2, 3))
  end
  return s
end
print(pass(300), pass(250))
local function fails(n, at)
  local s = 0
  for i = 1, n do
    local v = i
    if i == at then if i % 2 == 0 then v = false else v = nil end end
    s = s + assert(v, "boom " .. i)
  end
  return s
end
print(pcall(fails, 300, 250))
print(pcall(fails, 300, 251))
print(pcall(fails, 300, 1000))
local function fails2(n, at)
  local s = 0
  for i = 1, n do
    local v = i
    if i == at then v = nil end
    s = s + assert(v)
  end
  return s
end
print(pcall(fails2, 300, 270))
print(pcall(function() local t = setmetatable({}, { __tostring = function() return "custom" end }); for i = 1, 100 do assert(i < 90 or nil, t) end end))
print(select("#", assert(1, nil, nil)))
"##,
    },
    Prog {
        name: "p2_scheduler",
        small: true,
        src: r##"
-- P2: a richards-like scheduler loop: one hot loop whose body dispatches to several task kinds (class guards that fail in turn, so the
-- root trace exits at many sites and every site gets a side trace); the mix changes in phases (a kind appears, a kind vanishes, all
-- kinds in a rotating order). The tree must keep growing (no `Ineffective` flush of the root) and the results stay exact.
local Task = {}
Task.__index = Task
function Task.new(k) return setmetatable({ k = k, n = 0, acc = 0 }, Task) end
local Idle = setmetatable({}, { __index = Task }); Idle.__index = Idle
function Idle:run(i) self.n = self.n + 1; return i + 1 end
local Work = setmetatable({}, { __index = Task }); Work.__index = Work
function Work:run(i) self.acc = self.acc + i; return i * 3 + 1 end
local Hand = setmetatable({}, { __index = Task }); Hand.__index = Hand
function Hand:run(i) self.n = self.n + 2; self.acc = self.acc - 1; return i - 1 end
local Dev = setmetatable({}, { __index = Task }); Dev.__index = Dev
function Dev:run(i) if i % 2 == 0 then self.n = self.n + 1 end; return i + 2 end
local kinds = { Idle, Work, Hand, Dev }
local tasks = {}
for k = 1, 4 do tasks[k] = setmetatable({ n = 0, acc = 0 }, kinds[k]) end
local function sched(rounds, pat)
  local v = 0
  for r = 1, rounds do
    local t = tasks[pat(r)]
    v = t:run(v) % 100003
    if v % 7 == 0 then t.acc = t.acc + 1 end
  end
  return v
end
local out = {}
out[#out + 1] = sched(6000, function(r) return r % 4 + 1 end)
out[#out + 1] = sched(6000, function(r) return (r % 3 == 0) and 2 or 1 end)
out[#out + 1] = sched(6000, function(r) return (r // 50) % 4 + 1 end)
out[#out + 1] = sched(6000, function(r) return (r * r) % 4 + 1 end)
for k = 1, 4 do out[#out + 1] = tasks[k].n .. ":" .. tasks[k].acc end
print(table.concat(out, " "))
"##,
    },
    Prog {
        name: "p2_type_flip",
        small: true,
        src: r##"
-- P2: type-flip phases at the hot guards of rolled loops (a table constructor keeps the loops from being peeled): integer -> float ->
-- mixed -> string keys, at the same loads, and a persistent change (every iteration fails the recorded guard). Hot type-guard exits get
-- side traces; the results must stay identical to the interpreter's.
local function sum(t, n)
  local s = 0.0
  for i = 1, n do local o = { i }; s = s + t[i] * 0.5 + o[1] end
  return s
end
local function cat(t, n)
  local c = 0
  for i = 1, n do local o = { t[i] }; c = c + #tostring(o[1]) end
  return c
end
local n = 200
local ti, tf, tm, ts = {}, {}, {}, {}
for i = 1, n do ti[i] = i % 7; tf[i] = (i % 7) + 0.5; tm[i] = (i % 3 == 0) and (i + 0.25) or i; ts[i] = (i % 5 == 0) and "x" or i end
local total = 0.0
for r = 1, 40 do total = total + sum(ti, n) end
for r = 1, 40 do total = total + sum(tf, n) end
for r = 1, 40 do total = total + sum(tm, n) end
for r = 1, 40 do total = total + sum(ti, n) + sum(tf, n) end
local c = 0
for r = 1, 30 do c = c + cat(ti, n) end
for r = 1, 30 do c = c + cat(tm, n) + cat(ts, n) end
print(string.format("%.6f", total), c)
"##,
    },
    Prog {
        name: "p2_storm",
        small: true,
        src: r##"
-- P2: an exit storm that must still end: every iteration of the recorded loops leaves the trace through a guard that has a side trace
-- (a rotating receiver class, a persistent type change in a rolled loop, a guard that fails on every iteration of every entry). The
-- program must terminate with the interpreter's results, with and without the policy knobs.
local A = { v = 1 }; A.__index = A
local B = { v = 2 }; B.__index = B
local C = { v = 3 }; C.__index = C
local objs = { setmetatable({}, A), setmetatable({}, B), setmetatable({}, C) }
local function rot(n)
  local s = 0
  for i = 1, n do s = s + objs[i % 3 + 1].v end
  return s
end
local function flip(t, n)
  local s = 0
  for i = 1, n do local o = { t[i] }; s = s + o[1] end
  return s
end
local ti, tf = {}, {}
for i = 1, 100 do ti[i] = i; tf[i] = i + 0.5 end
local r1, r2 = 0, 0
for k = 1, 150 do r1 = r1 + rot(100) end
for k = 1, 5 do r2 = r2 + flip(ti, 100) end
for k = 1, 150 do r2 = r2 + flip(tf, 100) end
print(r1, r2)
"##,
    },
    Prog {
        name: "b1_tail",
        small: true,
        src: r##"
-- BUNDLE-1 (2): functions reached only by tail calls (`return f(x)`) count calls and get compiled: chains, mutual tail recursion, vararg and
-- C-function tail callees, `__call` callees, `CIST_TAIL` frames seen through debug.getinfo / traceback / hooks, errors raised in a tail callee, a
-- tail callee that yields, a callee redefined in the middle of the loop.
local N = 300
local out = {}
local function emit(...) out[#out + 1] = table.concat({ ... }, ",") end
local function leaf(x) return x * 2 + 1 end
local function mid(x) return leaf(x) end
local function tmid(x) return leaf(x + 1) end      -- tail call
local function top(x) return tmid(x) end            -- tail call to a tail caller
local s = 0
for i = 1, N do s = s + top(i) + mid(i) end
emit(s)
-- mutual tail recursion with a loop around it
local even, odd
function even(n) if n == 0 then return true end return odd(n - 1) end
function odd(n) if n == 0 then return false end return even(n - 1) end
local c = 0
for i = 1, N do if even(i % 9) then c = c + 1 end end
emit(c)
-- vararg and C-function tail callees
local function va(...) return select("#", ...), ... end
local function tva(a, b, c) return va(a, b, c) end
local function ts(x) return tostring(x) end
local function tm(a, b) return math.max(a, b) end
local acc = 0
for i = 1, N do
  local n, x, y, z = tva(i, i + 1, i + 2)
  acc = acc + n + x + y + z + #ts(i) + tm(i, 150)
end
emit(acc)
-- __call callee through a tail call
local callable = setmetatable({}, { __call = function(self, x) return x + 7 end })
local function tc(x) return callable(x) end
local cs = 0
for i = 1, N do cs = cs + tc(i) end
emit(cs)
-- debug.getinfo sees the tail frame
local function who() local i = debug.getinfo(1, "t") return i.istailcall and 1 or 0 end
local function twho() return who() end
local function nwho() local r = who() return r end
local ts_, ns_ = 0, 0
for i = 1, N do ts_ = ts_ + twho() ns_ = ns_ + nwho() end
emit(ts_, ns_)
-- traceback through a tail frame
local function boom(x) if x == 250 then error("boom " .. x) end return x end
local function tboom(x) return boom(x) end
local errs, last = 0, ""
for i = 1, N do
  local ok, m = pcall(tboom, i)
  if not ok then errs = errs + 1 last = m end
end
emit(errs, last)
local tb = select(2, xpcall(tboom, debug.traceback, 250))
emit(select(2, tb:gsub("%(%.%.%.tail calls%.%.%.%)", "")))
-- a coroutine whose tail callee yields
local function yl(x) coroutine.yield(x) return x + 1 end
local function tyl(x) return yl(x) end
local co = coroutine.wrap(function() local t = 0 for i = 1, 200 do t = t + tyl(i) end return t end)
local ys = 0
for i = 1, 200 do ys = ys + co() end
emit(ys, co())
-- call hooks count the same calls
local calls = 0
local function hook() calls = calls + 1 end
debug.sethook(hook, "c")
for i = 1, 40 do top(i) end
debug.sethook()
emit(calls)
-- the callee is redefined in the middle of the loop
local g1 = function(x) return x + 1 end
local g2 = function(x) return x * 3 end
local cur = g1
local function tg(x) return cur(x) end
local gs = 0
for i = 1, N do
  if i == 100 then cur = g2 end
  if i == 200 then cur = g1 end
  gs = gs + tg(i)
end
emit(gs)
print(table.concat(out, ";"))
"##,
    },
    Prog {
        name: "b8_sub_ring",
        small: true,
        src: r##"
-- HDRSUB (old B8, `bench/lib/sub`): loops with `string.sub` / `string.char` / `tostring` peeled (header-safe allocating library calls):
-- ring buffers of fresh strings, a fresh string carried to the next iteration, short and long results, results that are
-- equal / interned already, garbage strings (GC pressure), the string metatable and the source strings changing in the middle.
local N = 400
local out = {}
local function emit(...) out[#out + 1] = table.concat({ ... }, ",") end
local K = 16
local S = {}
for k = 1, K do S[k] = "the quick brown fox jumps over " .. k .. " dog" end
local P7 = {}
for k = 1, K do P7[k] = k % 7 + 1 end
local ring, acc = {}, 0
for o = 0, 24 do
  for k = 1, K do
    local r = string.sub(S[k], P7[k], P7[k] + 10)
    ring[k] = r
    acc = acc + #r
  end
end
local h = 0
for k = 1, K do h = h * 31 + #ring[k] + ring[k]:byte(1) end
emit(acc, h, ring[1], ring[K])
-- a fresh string carried over the back edge: the previous iteration's piece is used in this one
local prev, same, diff = "", 0, 0
for i = 1, N do
  local s = S[i % K + 1]
  local cur = s:sub(i % 5 + 1, i % 5 + 6)
  if cur == prev then same = same + 1 else diff = diff + 1 end
  prev = cur
end
emit(same, diff, prev)
-- results of both kinds (short up to 40 bytes, long above) and empty / negative / out-of-range bounds
local big = string.rep("0123456789", 12)
local kinds = 0
for i = 1, N do
  local a = big:sub(1, 30 + i % 30)
  local b = big:sub(-(i % 15), -1)
  local c = big:sub(i % 130 + 1, i % 7)
  kinds = kinds + #a + #b + #c
  if i == 200 then big = big .. big:sub(1, 20) end
end
emit(kinds)
-- two pieces swap through locals (PHIs of fresh strings)
local x, y = "a", "b"
local words = { "alpha beta", "gamma delta", "epsilon zeta" }
for i = 1, N do
  local w = words[i % 3 + 1]
  local nx = w:sub(1, 5)
  x, y = y, nx
  if i % 97 == 0 then x = x .. y end
end
emit(x, y)
-- `string.char` and `tostring` of numbers in a loop, the sub of the result consumed
local cs, ts = 0, 0
for i = 1, N do
  local c = string.char(65 + i % 26)
  local t = tostring(i * 3)
  local u = tostring(i + 0.5)
  cs = cs + #c + c:byte() + #t:sub(2, 3) + #u:sub(1, 2)
  ts = ts + #t + #u
end
emit(cs, ts)
-- garbage: many short-lived pieces with the collector stepping (never held)
local junk = 0
for i = 1, 3 * N do
  local j = ("garbage string number " .. i):sub(1, 10 + i % 12)
  junk = junk + #j
end
emit(junk)
-- the source changes class in the middle (short -> long string), and the string metatable's `sub`
local src = "short"
local total = 0
for i = 1, N do
  total = total + #string.sub(src, 1, 4)
  if i == 150 then src = string.rep("L", 100) end
  if i == 300 then src = 12345 end
end
emit(total)
local smt = getmetatable("")
local old = smt.__index
local m = 0
for i = 1, N do
  local s = "abcdefgh"
  m = m + #s:sub(2, 5)
  if i == 120 then smt.__index = { sub = function() return "xyz" end } end
  if i == 260 then smt.__index = old end
end
emit(m)
print(table.concat(out, ";"))
"##,
    },
    Prog {
        name: "b1_hdr",
        small: true,
        src: r##"
-- BUNDLE-1 (4): loops that contain a string `TLEN` or a read-only library call (`string.byte`, string compares, `type`, string-method
-- lookup, `math.random`) are peeled; the string metatable and the strings change in the middle of the loop.
local N = 300
local out = {}
local function emit(...) out[#out + 1] = table.concat({ ... }, ",") end
local s1, s2 = "hello world", string.rep("x", 60) .. "tail"
local a = 0
for i = 1, N do
  a = a + #s1 + #s2 + s1:len() + s2:byte(i % 20 + 1) + s1:byte(-1)
end
emit(a)
-- string compares in a loop (long and short strings, equal and not)
local l1, l2, l3 = string.rep("ab", 30), string.rep("ab", 30), string.rep("ab", 29) .. "ac"
local eq, lt, le = 0, 0, 0
for i = 1, N do
  if l1 == l2 then eq = eq + 1 end
  if l1 < l3 then lt = lt + 1 end
  if l3 <= l1 then le = le + 1 end
  if s1 == "hello world" then eq = eq + 2 end
  if i == 150 then l2 = l2 .. "z" end
  if i == 220 then l3 = l1 end
end
emit(eq, lt, le)
-- type() and a method of a string receiver, with the string metatable changed
local smt = getmetatable("")
local old_len = string.len
local mk = 0
local strs = { "a", "bb", "ccc", "dddd" }
for i = 1, N do
  local s = strs[i % 4 + 1]
  mk = mk + s:len() + (type(s) == "string" and 1 or 0) + #s
  if i == 100 then smt.__index = { len = function() return 100 end, byte = string.byte, sub = string.sub } end
  if i == 200 then smt.__index = string end
end
emit(mk)
-- the string method table is edited
local up = 0
for i = 1, N do
  local s = "abc"
  up = up + s:len()
  if i == 120 then string.len = function() return 50 end end
  if i == 240 then string.len = old_len end
end
emit(up)
-- math.random with a seed, and byte ranges
math.randomseed(12345)
local r = 0
for i = 1, N do
  r = r + math.random(1, 100) + math.random(5, 6) + (("abcdef"):byte(1, 3))
end
emit(r)
local bs = 0
for i = 1, N do
  local x, y, z = s1:byte(1, 3)
  bs = bs + x + y + z + (s2:byte(i % 64 + 1) or 0)
end
emit(bs)
-- a loop whose string changes length with a table length and a rawlen next to it
local tl, t = 0, { 1, 2, 3 }
local buf = ""
for i = 1, N do
  buf = (i % 7 == 0) and "" or (buf .. "x")
  tl = tl + #buf + #t + rawlen(t) + #("k" .. i)
  if i % 10 == 0 then t[#t + 1] = i end
end
emit(tl)
print(table.concat(out, ";"))
"##,
    },
    Prog {
        name: "b1_tail_only",
        small: true,
        src: r##"
-- BUNDLE-1 (2): `leaf` is reached only by a tail call: with `tailhook` the call counter of the interpreter's tail call compiles it.
local function leaf(x) if x % 3 == 0 then return x + 1 end return x * 2 end
local function mid(x) return leaf(x) end
local s = 0
for i = 1, 20000 do s = s + mid(i) end
print(s)
"##,
    },
    Prog {
        name: "b1_nest",
        small: true,
        src: r##"
-- BUNDLE-1 (1): a loop nest (escape-time iteration inside two counted loops) whose trace formation must not depend on the start gate.
local function mandelbrot(size)
  local sum, inside = 0, 0
  for y = 0, size - 1 do
    local ci = 2.0 * y / size - 1.0
    local bitNum, byteAcc = 0, 0
    for x = 0, size - 1 do
      local zr, zi, zrzr, zizi = 0.0, 0.0, 0.0, 0.0
      local cr = 2.0 * x / size - 1.5
      local z, escape = 0, 0
      while escape == 0 and z < 50 do
        zr = zrzr - zizi + cr
        zi = 2.0 * zr * zi + ci
        zrzr = zr * zr
        zizi = zi * zi
        if zrzr + zizi > 4.0 then escape = 1 end
        z = z + 1
      end
      if escape == 0 then inside = inside + 1 end
      byteAcc = byteAcc * 2 + escape
      bitNum = bitNum + 1
      if bitNum == 8 then
        sum = sum + byteAcc
        byteAcc, bitNum = 0, 0
      end
    end
  end
  return sum, inside
end
local s, n = 0, 0
for i = 1, 1 do local a, b = mandelbrot(400) s = s + a n = n + b end
print(s, n)
"##,
    },
    Prog {
        name: "b5_sink",
        small: true,
        src: r##"
-- BUNDLE-5 (`sinkmt`, `sinkside`): allocation sinking of tables that get a metatable and in side / stitched traces. Vector code whose
-- temporaries never leave the trace, branches that make side traces (the temporary lives across them), polymorphic shapes (exits that
-- materialise the table), a temporary that is stored / returned / passed to a native in some iterations (escapes), a metatable that
-- gets `__gc` / `__index` late, a table that is compared by identity, errors in the middle, a callee that allocates in the side path.
local V = {}
V.__index = V
local function vec(x, y) return setmetatable({ x = x, y = y }, V) end
function V:add(o) return vec(self.x + o.x, self.y + o.y) end
function V:scale(k) return vec(self.x * k, self.y * k) end
function V:dot(o) return self.x * o.x + self.y * o.y end
local acc = 0
local a, b = vec(1, 2), vec(0.5, 0.25)
for i = 1, 3000 do
  local c = a:add(b):scale(0.5)
  acc = acc + c:dot(a)
  if i % 7 == 0 then acc = acc + c.x end                 -- a branch: side trace with the temporary live
  if i % 11 == 0 then b = vec(b.y, b.x) end
end
print(acc)
-- polymorphic operand shapes: the guard after the allocation fails for half of the iterations
local p3 = setmetatable({ x = 1, y = 2, z = 3 }, V)
local p2 = vec(4, 5)
local s = 0
for i = 1, 3000 do
  local o = (i % 2 == 0) and p2 or p3
  local t = vec(o.x + 1, o.y + 1)
  s = s + t.x * t.y
end
print(s)
-- escapes: stored, returned, passed to a native, compared
local keep = {}
local same = 0
for i = 1, 2000 do
  local t = vec(i, i + 1)
  if i % 100 == 0 then keep[#keep + 1] = t end            -- stored: the table must exist
  if i % 250 == 0 then print(rawequal(t, t), type(t), tostring(getmetatable(t) == V)) end
  local u = t
  if u == t then same = same + 1 end
end
print(#keep, keep[3].x, keep[3].y, same)
-- a metatable gets __gc late: the guard of the sunk setmetatable must leave the trace
local G = { __index = V }
local function gv(x) return setmetatable({ x = x, y = 0 }, G) end
local fin = 0
local g = 0
for i = 1, 3000 do
  local t = gv(i)
  g = g + t.x
  if i == 1500 then G.__gc = function() fin = fin + 1 end end
end
collectgarbage()
collectgarbage()
print(g, fin > 0)
-- __index added to the sunk table's metatable: a lookup of a missing key on the sunk table resolves through it
local M = { }
local function mv(x) return setmetatable({ x = x }, M) end
local q = 0
for i = 1, 2000 do
  local t = mv(i)
  q = q + t.x + (t.k or 0)
  if i == 1000 then M.__index = { k = 100 } end
end
print(q)
-- errors in the middle of a loop that holds temporaries
local function risky(n)
  local r = 0
  for i = 1, n do
    local t = vec(i, 2 * i)
    r = r + t:dot(t)
    if i == n - 3 then t = nil; r = r + t.x end
  end
  return r
end
print(pcall(risky, 20))
print(pcall(risky, 20))
-- locals and upvalues hold the table past the loop
local last
for i = 1, 2000 do
  local t = vec(i, -i)
  last = (i % 500 == 0) and t or last
end
print(last.x, last.y, getmetatable(last) == V)
-- a coroutine that yields while the temporary is live
local co = coroutine.wrap(function()
  for i = 1, 1000 do
    local t = vec(i, i)
    coroutine.yield(t:dot(t))
  end
end)
local cs = 0
for i = 1, 1000 do cs = cs + co() end
print(cs)
"##,
    },
    Prog {
        name: "ba_nilset",
        small: true,
        src: r##"
-- BUNDLE-A item 2: tier-1 stores into a node of an existing key whose value is nil (SETFIELD / SETTABUP) and into an empty array slot of a
-- table with a metatable (SETI / SETTABLE), in hot functions, with the situation changed in the middle of the loop.
local log = {}
-- 1. a metatable gets `__newindex` through the same hot store that fills nil slots: the cached "no __newindex" bit of the metatable must
-- be dropped by the store into the nil-valued `__newindex` node
local P = {}
local function setfield(t, v) t.x = v end
local function setmeta(m, f) m.__newindex = f end
local obj = setmetatable({ x = 0, y = 1 }, P)
setmeta(P, false); setmeta(P, nil)           -- the key exists in P with a nil value
local cnt = 0
for i = 1, 600 do
  setfield(obj, nil)
  setfield(obj, i)                              -- nil slot of obj; P has no __newindex
  if i == 200 then setmeta(P, function(t, k, v) cnt = cnt + 1; rawset(t, k, v) end) end
  if i == 400 then setmeta(P, nil) end
  rawset(obj, "x", nil)
end
print("newindex", cnt, obj.x)
-- 2. fields that alternate between nil and a value in a plain table and in a table with a metatable without `__newindex`
local C = {}
C.__index = C
local function set3(t, a, b, c) t.a = a; t.b = b; t.c = c end
local plain, inst = { a = 1, b = 2, c = 3 }, setmetatable({ a = 1, b = 2, c = 3 }, C)
local sum = 0
for i = 1, 500 do
  set3(plain, nil, i, nil); set3(inst, nil, i, nil)
  set3(plain, i, nil, i); set3(inst, i, nil, i)
  sum = sum + (plain.a or 0) + (inst.c or 0) + (plain.b or -1) + (inst.b or -1)
end
print("alt", sum)
-- 3. upvalue-table stores (SETTABUP): a global that is set to nil and set again, with and without a metatable on _ENV
local env = setmetatable({}, { __index = _G })
local function gset(v) gslot = v end
local f = load("local gset = ... ; gslot = nil; for i = 1, 300 do gslot = i; gslot = nil end; gslot = 7; return gslot", "=g", "t", env)
print("settabup", f(), rawget(env, "gslot"))
-- 4. empty slots of the array part: table.pack / constructor with nils, a metatable without and with `__newindex`
local function seti(t, v) t[2] = v end
local function settable(t, k, v) t[k] = v end
local plain_mt = setmetatable({ 1, 2, 3 }, C)
local ni_log = 0
local ni_mt = setmetatable({ 1, 2, 3 }, { __newindex = function(t, k, v) ni_log = ni_log + 1; rawset(t, k, v and v * 10) end })
local s4 = 0
for i = 1, 300 do
  plain_mt[2] = nil; ni_mt[2] = nil
  rawset(ni_mt, 2, nil)
  seti(plain_mt, i)
  settable(plain_mt, 1, nil); settable(plain_mt, 1, i)
  seti(ni_mt, i)
  s4 = s4 + plain_mt[2] + (rawget(ni_mt, 2) or 0)
  rawset(ni_mt, 2, nil)
  settable(ni_mt, 3, nil)
  rawset(ni_mt, 3, nil)
  settable(ni_mt, 3, i)
  s4 = s4 + (rawget(ni_mt, 3) or 0)
end
print("array", s4, ni_log)
-- the cached bit goes stale in the other direction: the metatable gets `__newindex` only after the array slots were filled once
local Q = {}
local q = setmetatable({ 1, 2, 3 }, Q)
local qn = 0
for i = 1, 400 do
  q[2] = nil
  seti(q, i)
  q[2] = nil
  if i == 150 then Q.__newindex = function(t, k, v) qn = qn + 1; rawset(t, k, v) end end
  if i == 300 then Q.__newindex = nil end
end
print("late", qn)
-- 5. collectable values into nil slots of old tables (write barrier), in both collector modes
local holder = { a = nil, b = nil, c = nil }
local arr = { nil, nil, nil, nil }
local function fill(i)
  holder.a = { i }; holder.b = "s" .. i; holder.c = function() return i end
  arr[1] = { i }; arr[2] = "t" .. i
end
local function clear() holder.a = nil; holder.b = nil; holder.c = nil; arr[1] = nil; arr[2] = nil end
local keep = 0
for mode = 1, 2 do
  collectgarbage(mode == 1 and "incremental" or "generational")
  for round = 1, 60 do
    collectgarbage("step", 0)
    fill(round)
    collectgarbage("step", 0)
    collectgarbage("step", 0)
    keep = keep + holder.a[1] + #holder.b + holder.c() + arr[1][1] + #arr[2]
    clear()
  end
end
collectgarbage()
print("barrier", keep)
-- 6. event-name keys stored into nil slots of a table that is another table's metatable
local M = {}
local obj2 = setmetatable({}, M)
M.__index = function(t, k) return k .. "!" end
M.__index = nil
local ev = 0
for i = 1, 600 do
  local a = obj2.zz
  ev = ev + (a == nil and 1 or #a)
  if i % 3 == 0 then M.__index = function(t, k) return k .. "?" end else M.__index = nil end
end
print("events", ev)
"##,
    },
    Prog {
        name: "g1_fmt",
        small: true,
        src: r##"
-- BUNDLE-G1 item 4: string.format("%s", s) of a short string is folded to s in traces; every guard failure runs the real call.
local fmt = string.format
local N = 300
local S = {}
for k = 1, 40 do S[k] = "str number " .. k end
local acc = 0
for r = 1, N do
  for k = 1, #S do local x = fmt("%s", S[k]); acc = acc + #x + (x == S[k] and 1 or 0) end
end
print("short", acc)
-- the argument changes type: numbers, a long string, nil (error), a table with __tostring, a boolean, a string with a zero byte
local mt = setmetatable({}, { __tostring = function() return "TOSTR" end })
local mixed = { "a", 5, 2.5, "b", true, "c", mt, string.rep("L", 60), "d", "z\0z", -7, "e" }
local out = {}
for r = 1, 120 do
  for k = 1, #mixed do out[k] = fmt("%s", mixed[k]) end
end
print("mixed", table.concat(out, ","):len(), out[7], out[8]:len(), #out[10])
local ok, e = pcall(function() for r = 1, 50 do local v = (r == 40) and nil or "q"; fmt("%s", v) end end)
print("nilarg", ok, e)
-- the format changes: another format string, extra arguments, a different format of the same site, the function replaced
local fs = { "%s", "%s", "%s", "[%s]", "%s", "%5s", "%s", "%.2s", "%s" }
local acc2 = 0
for r = 1, 200 do
  for k = 1, #fs do acc2 = acc2 + #fmt(fs[k], "hello") end
end
print("fmts", acc2)
local ex = 0
for r = 1, 200 do ex = ex + #fmt("%s", "ab", "cd") + (r > 100 and #fmt("%s%s", "ab", "cd") or 0) end
print("extra", ex)
local f2 = string.format
local sf = string.format
local acc3 = 0
for r = 1, 300 do
  if r == 150 then string.format = function(f, s) return "<" .. s .. ">" end end
  acc3 = acc3 + #string.format("%s", "wxyz")
end
string.format = sf
print("replaced", acc3)
local acc4 = 0
for r = 1, 300 do
  local f = fmt
  if r == 200 then f = function(a, b) return "no" end end
  acc4 = acc4 + #f("%s", "abc")
end
print("callee", acc4)
local tr = 0
for r = 1, 200 do tr = tr + select("#", fmt("%s", "x")) + #(fmt("%s", "yy")) end
print("tail", tr)
"##,
    },
    Prog {
        name: "g1_streq",
        small: true,
        src: r##"
-- BUNDLE-G1 item 5: == / ~= of two long strings: same object, different lengths, differing only at the head / middle / tail, equal content in two objects.
local P = string.rep("the quick brown fox ", 4)
local A, B = {}, {}
for k = 1, 64 do A[k] = P .. "number " .. k; B[k] = P .. "number " .. (k % 7 == 0 and k or k + 1000) end
local C = {}
for k = 1, 64 do C[k] = A[k]:sub(1, 30) .. "X" .. A[k]:sub(32) end    -- differs in the middle
local D = {}
for k = 1, 64 do D[k] = "Y" .. A[k]:sub(2) end                        -- differs at the head
local E = {}
for k = 1, 64 do E[k] = P .. "number " .. k .. "!" end               -- longer
local F = {}
for k = 1, 64 do F[k] = A[k]:sub(1, #A[k] - 1) .. "~" end            -- differs in the last byte
local G = {}
for k = 1, 64 do G[k] = (P .. "number "):rep(1) .. k end             -- equal content, a different object
local sets = { A, B, C, D, E, F, G, A }
local cnt = { 0, 0, 0, 0, 0, 0, 0, 0 }
for r = 1, 200 do
  for s = 1, #sets do
    local X = sets[s]
    for k = 1, 64 do
      if A[k] == X[k] then cnt[s] = cnt[s] + 1 end
      if A[k] ~= X[k] then cnt[s] = cnt[s] + 100 end
    end
  end
end
print(table.concat(cnt, ","))
-- a long string next to a short one, a non-string, strings that grow, an 8-byte tail window crossing equal bytes
local m = { A[1], "short", 5, A[2], true, B[3], P .. "tail", P .. "tail" }
local mc = 0
for r = 1, 200 do
  for i = 1, #m do for j = 1, #m do if m[i] == m[j] then mc = mc + 1 end end end
end
print("mixed", mc)
local L1, L2 = {}, {}
for k = 1, 50 do
  local base = string.rep("ab", 30)
  L1[k] = base .. string.rep("c", k % 9)
  L2[k] = base .. string.rep("c", (k + 3) % 9)
end
local lc = 0
for r = 1, 200 do for k = 1, 50 do if L1[k] == L2[k] then lc = lc + 1 elseif L1[k] ~= L2[k] then lc = lc + 10 end end end
print("grow", lc)
-- strings that share the last eight bytes and differ before them
local H1, H2 = {}, {}
for k = 1, 50 do
  H1[k] = string.rep("m", 30) .. "a" .. string.rep("z", 20) .. "-end-end"
  H2[k] = string.rep("m", 30) .. (k % 2 == 0 and "a" or "b") .. string.rep("z", 20) .. "-end-end"
end
local hc = 0
for r = 1, 200 do for k = 1, 50 do if H1[k] == H2[k] then hc = hc + 1 else hc = hc + 5 end end end
print("sametail", hc)
"##,
    },
    Prog {
        name: "g1_tget",
        small: true,
        src: r##"
-- BUNDLE-G1 item 1: tier-1 GETTABLE with an integer key outside the array part probes the hash part inline (towers: piles[0]).
local function get(t, k) return t[k] end
local function sum(t, ks, n)
  local s = 0
  for i = 1, n do local v = get(t, ks[i]); if v ~= nil then s = s + (type(v) == "number" and v or #tostring(v)) end end
  return s
end
-- 1. hash-part integer keys: zero, negative, huge, colliding (equal modulo the node count), nil-valued after removal, migrating to the array part
local h = { [0] = 5, [-1] = 6, [1 << 40] = 7, [math.maxinteger] = 8, [math.mininteger] = 9, [100] = 10, [1000] = 11, [3] = 12, [7] = 13, [11] = 14 }
local ks = { 0, -1, 1 << 40, math.maxinteger, math.mininteger, 100, 1000, 3, 7, 11, 5, -5, 1, 2, 4, 2.0, 0.0, "a", 1.5 }
local tot = 0
for r = 1, 60 do
  tot = tot + sum(h, ks, #ks)
  if r % 11 == 0 then h[0] = nil; h[7] = nil; h[1 << 40] = nil end
  if r % 13 == 0 then h[0] = r; h[7] = r; h[1 << 40] = r; h[1] = r; h[2] = r; h[4] = r end
end
print("hash", tot)
-- 2. a miss falls through to __index (table and function), a hit does not, a nil-valued dead key is a miss
local base = { [0] = "b0", [-1] = "bm", [50] = "b50" }
local hits = 0
local m = setmetatable({ [0] = "i0", [8] = "i8", [-1] = false }, { __index = function(t, k) hits = hits + 1; return "f" .. tostring(k) end })
local m2 = setmetatable({ [0] = "x0", [9] = "x9" }, { __index = base })
local ks2 = { 0, -1, 8, 9, 50, 51, 1 << 33 }
local out = {}
for r = 1, 50 do
  for i = 1, #ks2 do out[#out + 1] = tostring(get(m, ks2[i])) .. tostring(get(m2, ks2[i])) end
  if r % 10 == 0 then m[0] = nil; m2[0] = nil; m[8] = nil end
  if r % 17 == 0 then m[0] = "back"; m2[0] = "back2" end
  if #out > 300 then out = { #out } end
end
print("index", hits, #out, out[#out])
-- 3. the key changes kind and the table changes shape at one site; non-table receivers and error messages
local mixed = { a = 1, [10] = 2, [2.5] = 3, [0] = 4 }
local mk = { "a", 10, 2.5, 0, "a", 10, 0, 11, 3.5, 2^53, -0.0 }
local t3 = 0
for r = 1, 60 do
  for i = 1, #mk do t3 = t3 + (get(mixed, mk[i]) or 0) end
  if r == 30 then for i = 1, 20 do mixed[i] = i end end
  if r == 45 then for i = 1, 20 do mixed[i] = nil end end
end
print("mixed", t3)
-- float keys whose bit pattern equals an integer key of the hash part (5e-324 has the bits of 1) must not hit it
local fh = { [1] = "one", [2] = "two", [0] = "zero" }
local fk = { 5e-324, 1e-323, 0.0, 1, 2, 0, "x" }
local fo = {}
for r = 1, 40 do for i = 1, #fk do fo[i] = tostring(get(fh, fk[i])) end end
print("floatbits", table.concat(fo, ","))
local e = 0
for i = 1, 100 do
  local ok = pcall(get, (i % 3 == 0) and 5 or ((i % 3 == 1) and "str" or nil), 0)
  if not ok then e = e + 1 end
end
print("errors", e, get("abc", 0), get(setmetatable({}, { __index = function() return 1 end }), 0))
-- 4. towers shape: piles[0] in the hash part of a table with two array slots; collectable values stored in a loop with GC steps
local piles = {}
piles[1] = { n = 1 }; piles[2] = { n = 2 }; piles[0] = { n = 0 }
local cnt = 0
for r = 1, 200 do
  for i = 0, 2 do local p = get(piles, i); cnt = cnt + p.n end
  piles[0] = { n = r % 3 }
  if r % 25 == 0 then collectgarbage("step", 0) end
end
print("piles", cnt)
"##,
    },
    Prog {
        name: "bb_tset",
        small: true,
        src: r##"
-- BUNDLE-B item 2: tier-1 SETTABLE with a short-string key or an integer key in the hash part into an existing node, in hot functions, with
-- the key kind, the table shape and the metatable changed in the middle of the loops.
local function put(t, k, v) t[k] = v end
local function bump(t, k) t[k] = (t[k] or 0) + 1 end
-- string hashes are seeded per process, so an order that involves string keys is printed sorted; integer-only tables print the raw `pairs` order
local function keys(t, raw)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  if not raw then table.sort(ks) end
  return table.concat(ks, ",")
end
-- 1. dynamic string keys: existing, new, nil-valued, colliding, long strings, and other key types through the same instruction
local names = {}
for i = 1, 40 do names[i] = "key" .. i end
names[41] = string.rep("L", 50)                -- long string key
names[42] = 7                                  -- integer key (array part / hash part)
names[43] = 2.0                                -- float key with an integer value
names[44] = 1.5
names[45] = true
names[46] = names                              -- table key
local c = {}
for round = 1, 30 do
  for i = 1, #names do bump(c, names[i]) end
  if round % 7 == 0 then for i = 1, 40, 3 do put(c, names[i], nil) end end
end
local tot = 0
for i = 1, #names do tot = tot + (c[names[i]] or 0) end
print("strkeys", tot, keys(c):len())
-- 2. integer keys in the hash part: sparse, negative, huge, boundary of the array part, becoming array keys after growth
local h = { [0] = "z", [-1] = "m", [1 << 40] = "big", [math.maxinteger] = "max", [math.mininteger] = "min", [100] = "h", [1000] = "k" }
local ik = { 0, -1, 1 << 40, math.maxinteger, math.mininteger, 100, 1000, 5, 6, 7, -2, 3 << 33 }
local s2 = 0
for round = 1, 40 do
  for i = 1, #ik do
    put(h, ik[i], round + i)
    s2 = s2 + (h[ik[i]] or 0)
  end
  if round % 9 == 0 then for i = 1, #ik, 2 do put(h, ik[i], nil) end end
end
print("intkeys", s2, keys(h, true))
-- 3. the key changes kind at one store site
local function site(t, k, v) t[k] = v end
local mixed = { a = 1, [10] = 2, [2.5] = 3 }
local ks = { "a", 10, 2.5, "a", 10, 2.5, "b", 11, 3.5 }
for r = 1, 60 do for i = 1, #ks do site(mixed, ks[i], r) end end
print("mixed", keys(mixed))
-- 4. errors and metamethods keep their meaning: nil / NaN keys, __newindex on absent keys only, added and removed in the middle
local nic = 0
local mt = setmetatable({ x = 1, [50] = 1 }, { __newindex = function(t, k, v) nic = nic + 1; rawset(t, k, v) end })
local errs = 0
for i = 1, 300 do
  put(mt, "x", i); put(mt, 50, i)                -- existing: no __newindex
  put(mt, "y" .. (i % 5), i)                     -- absent the first time: __newindex, then existing
  if i % 50 == 0 then rawset(mt, "x", nil); rawset(mt, 50, nil) end
  if i % 100 == 0 then local ok = pcall(put, mt, nil, 1); if not ok then errs = errs + 1 end; ok = pcall(put, mt, 0 / 0, 1); if not ok then errs = errs + 1 end end
end
print("newindex", nic, errs, keys(mt))
-- 5. collectable values stored into old tables (write barrier) in both collector modes
local old = { s = false, [1 << 41] = false }
collectgarbage()
local kept = 0
for mode = 1, 2 do
  collectgarbage(mode == 1 and "incremental" or "generational")
  for r = 1, 80 do
    collectgarbage("step", 0)
    put(old, "s", { r })
    put(old, 1 << 41, "v" .. r)
    collectgarbage("step", 0)
    kept = kept + old.s[1] + #old[1 << 41]
  end
end
collectgarbage()
print("barrier", kept)
-- 6. pairs order after stores through SETTABLE equals the interpreter's (node placement is the table's, not the store's)
local o = {}
for i = 1, 12 do put(o, i * 1000, i) end
for i = 1, 12, 2 do put(o, i * 1000, nil) end
for i = 1, 12 do put(o, i * 1000, i * 2) end
for i = 1, 8 do put(o, -i * 77, i) end
for i = 1, 8, 3 do put(o, -i * 77, i + 100) end
print("order", keys(o, true))
"##,
    },
    Prog {
        name: "g2_pcall_depth",
        small: true,
        src: r##"
-- BUNDLE-G2: the C-stack guard of an inlined pcall is checked once per trace entry (it is loop invariant). A loop trace recorded at a
-- shallow C depth is entered again from deep pcall recursion: the entry check must fail near LUAI_MAXCCALLS and the interpreter's
-- pcall must raise "C stack overflow" at the same depth as without the trace.
local function body(i) return i + 1 end
local function mayfail(i, k) if i == k then error("e" .. i) end return i * 2 end
local function loop(n, k)
  local s = 0
  for i = 1, n do
    local ok, v = pcall(body, i); s = s + v
    local ok2, v2 = pcall(mayfail, i, k); s = s + (ok2 and v2 or #v2)
    local ok3, v3 = xpcall(body, print, i); s = s + v3
  end
  return s
end
local function deep(d, n, k)
  if d == 0 then return loop(n, k) end
  local ok, v = pcall(deep, d - 1, n, k)
  if not ok then error(v, 0) end
  return v
end
local out = {}
for _, d in ipairs{0, 1, 2, 5, 40, 120, 170, 180, 184, 186, 188, 190, 192, 194, 196, 198, 199, 200, 210} do
  local ok, v = pcall(deep, d, 200, 100000)
  out[#out + 1] = d .. ":" .. (ok and tostring(v) or ("E:" .. tostring(v):gsub("0x%x+", "P"):sub(1, 40)))
end
print(table.concat(out, " "))
-- the error inside the body at several depths
for _, d in ipairs{0, 60, 150, 189, 191, 195} do
  local ok, v = pcall(deep, d, 200, 150)
  print(d, ok, ok and v or (tostring(v):gsub("0x%x+", "P")))
end
-- many iterations at a fixed depth that is fine
print(deep(100, 3000, 100000), deep(100, 3000, 1500))
"##,
    },
    Prog {
        name: "h1_hash_int",
        small: true,
        src: r##"
-- BUNDLE-H1: integer keys in the hash part (0, negative, huge, float with an integer value) read and written by traces; migration between
-- the array and the hash part, growth in the middle of a hot loop, nil values, __index / __newindex on the misses.
local function rd(t, k) return t[k] end
local function wr(t, k, v) t[k] = v end
local function pass(t, keys, n)
  local s = 0
  for i = 1, n do
    for j = 1, #keys do
      local v = rd(t, keys[j])
      if v ~= nil then s = s + (type(v) == "number" and v or #tostring(v)) end
    end
  end
  return s
end
local function churn(t, keys, n)
  for i = 1, n do
    for j = 1, #keys do
      local k = keys[j]
      t[k] = (t[k] or 0) + i
    end
  end
  local s = 0
  for j = 1, #keys do s = s + (t[keys[j]] or -1) end
  return s
end
local keys = { 0, -1, 1 << 40, 1, 2.0, -(1 << 33), 3, 100, 7 }
local t = { [0] = 5, [-1] = 6, [1 << 40] = 7, [-(1 << 33)] = 9 }
t[1] = 1; t[2] = 2
print(pass(t, keys, 120), churn(t, keys, 120), t[0], t[-1], t[1 << 40], t[2], t[100], t[7])
-- the same loop after the array part grows over the hash keys (migration) and after the table loses them
for i = 3, 70 do t[i] = i end
print(pass(t, keys, 120), churn(t, keys, 60), t[3], t[7], t[100])
for i = 3, 70 do t[i] = nil end
t[100] = nil
print(pass(t, keys, 120), churn(t, keys, 60), t[3], t[100])
-- a hot loop that grows the table (insertions rehash the node vector under the traces)
local function grow(t, n)
  local s = 0
  for i = 1, n do
    t[-i] = i
    t[0] = (t[0] or 0) + 1
    s = s + t[-i] + t[0]
    if i % 7 == 0 then t[-i] = nil end
  end
  return s
end
local g = {}
print(grow(g, 300), grow(g, 120), g[0], g[-1], g[-7], g[-8])
-- nil values with a metatable: __index answers the misses, __newindex the stores to absent or nil-valued keys
local log = {}
local mt = {
  __index = function(_, k) log[#log + 1] = "i" .. k; return k * 2 end,
  __newindex = function(tt, k, v) log[#log + 1] = "n" .. k; rawset(tt, k, v) end,
}
local m = setmetatable({ [0] = 1, [-4] = 2, [(1 << 40)] = 3 }, mt)
local function mloop(m, n)
  local s = 0
  for i = 1, n do
    s = s + m[0] + m[-4] + m[1 << 40]
    if i % 25 == 0 then m[0] = nil; m[-4] = nil end
    if i % 25 == 10 then m[0] = i; m[-4] = -i end
    s = s + m[0] + m[-4]
  end
  return s
end
print(mloop(m, 200), #log, log[1], log[#log])
-- a metatable that appears between runs of the hot loops: a nil-valued integer-key node is not a raw hit once `__index` / `__newindex`
-- exist, so the traces of the earlier runs must exit at their metatable guards
local nt = { [0] = 5, [-1] = 6, [1 << 40] = 7 }
nt[0] = nil; nt[-1] = nil
local nkeys = { 0, -1, 1 << 40 }
print(pass(nt, nkeys, 150), pass(nt, nkeys, 150))
setmetatable(nt, { __index = function(_, k) return 11 end })
print(pass(nt, nkeys, 150), pass(nt, nkeys, 150))
local mt2 = { [0] = 1, [-1] = 2, [1 << 40] = 3 }
mt2[0] = nil; mt2[-1] = nil
print(churn(mt2, nkeys, 150), churn(mt2, nkeys, 150))
mt2[0] = nil; mt2[-1] = nil
local nlog = 0
setmetatable(mt2, { __newindex = function(tt, k, v) nlog = nlog + 1; rawset(tt, k, v + 1000) end })
print(churn(mt2, nkeys, 150), nlog, mt2[0], mt2[-1])
-- stores only (no read to share the metatable guard with): every store goes to a nil-valued node
local wlog = 0
local function wonly(t, keys, n)
  for i = 1, n do
    for j = 1, #keys do
      local k = keys[j]
      t[k] = i
      t[k] = nil
    end
  end
  return n
end
local wt = { [0] = 1, [-1] = 2 }
wt[0] = nil; wt[-1] = nil
local wkeys = { 0, -1 }
print(wonly(wt, wkeys, 150), wonly(wt, wkeys, 150), wt[0], wt[-1])
setmetatable(wt, { __newindex = function(tt, k, v) wlog = wlog + 1; rawset(tt, k, v) end })
print(wonly(wt, wkeys, 150), wlog, wt[0], wt[-1])
-- float keys with integer values and the integer 2^53
local f = { [0] = "a", [2^53 | 0] = "b" }
local function fl(f, n)
  local c = 0
  for i = 1, n do
    c = c + #f[0.0] + #f[2^53] + #(f[-0.0] or "")
    f[0.0] = f[0.0] .. ""
  end
  return c
end
print(fl(f, 150))
"##,
    },
    Prog {
        name: "h1_hash_int_many",
        small: false,
        src: r##"
-- BUNDLE-H1: one trace serves every integer key of a big hash part (a wrong main position or a chain walk that stops early is a run-time miss,
-- i.e. an exit per key): 200 negative keys and 0 read and updated in a hot loop.
local function fill(t) for i = 1, 200 do t[-i] = i end t[0] = 7 end
local function hot(t, n)
  local s = 0
  for r = 1, n do
    for i = 0, 200 do
      local k = -i
      s = s + t[k]
      t[k] = t[k] + 1
    end
  end
  return s
end
local t = {}
fill(t)
print(hot(t, 40), t[0], t[-1], t[-200])
local u = {}
for i = 1, 40 do u[-i * 7] = i end
u[0] = 1
local function hot2(t, n)
  local s = 0
  for r = 1, n do
    for i = 0, 40 do s = s + t[-i * 7] end
  end
  return s
end
print(hot2(u, 60))
"##,
    },
    Prog {
        name: "h2_fmat",
        small: true,
        src: r##"-- BUNDLE-H2: exits inside inlined frames (type changes in the leaf) that the inline stub restores itself: what the rebuilt CallInfo
-- nodes show to the rest of the program (callers' current lines, tracebacks, results wanted, tail frames, errors, hooks installed later).
local hookn = 0
local function hk() hookn = hookn + 1 end
local function leaf(x, mode)
  local y = x + 1
  if mode == 1 then
    local a, b = debug.getinfo(2, "l"), debug.getinfo(3, "l")
    return a.currentline, b and b.currentline or -1
  elseif mode == 2 then
    local tb = debug.traceback("t", 1)
    return #tb, select(2, tb:gsub("\n", "\n"))
  elseif mode == 3 then
    return pcall(error, "boom")
  elseif mode == 4 then
    return select("#", y, y, y), y
  end
  return y, y * 2
end
local function mid(x, mode)
  local a, b = leaf(x, mode)
  return a, b
end
local function tl(x, mode)
  return leaf(x, mode)
end
local function wrap(x, mode)
  local r1 = mid(x, mode)
  local r2, r3 = tl(x, mode)
  local r4 = (tl(x, mode))
  return r1, r2, r3, r4
end
local R1, R2, R3, R4 = {}, {}, {}, {}
for i = 1, 900 do
  local x = i
  if i % 61 == 0 then x = i + 0.5 end
  local mode = 0
  if i > 300 then mode = i % 5 end
  if i == 700 then debug.sethook(hk, "l") end
  if i == 760 then debug.sethook() end
  local r1, r2, r3, r4 = wrap(x, mode)
  R1[i], R2[i], R3[i], R4[i] = r1, r2, r3, r4
end
local acc = {}
for i = 1, 900 do
  acc[i] = tostring(R1[i]) .. "," .. tostring(R2[i]) .. "," .. tostring(R3[i]) .. "," .. tostring(R4[i])
end
print(#acc, acc[1], acc[301], acc[350], acc[600], acc[722], acc[900], hookn)
local h = 0
for i = 1, #acc do for j = 1, #acc[i] do h = (h * 31 + acc[i]:byte(j)) % 1000000007 end end
print(h)
"##,
    },
    Prog {
        name: "h1_towers_piles",
        small: true,
        src: r##"-- BUNDLE-H1: towers of Hanoi (bench/awfy/towers.lua, 3 rounds of 13 disks) with piles keyed 1..3 and `other = 3 - from - to` = 0 (key 0 in
-- the hash part, nil-vs-table values): recursion through inlined frames; the exits inside inlined frames of function-entry / continuation
-- traces get side traces with virtual frames.
local N = 3
local Disk = {}
Disk.__index = Disk
local function newDisk(size) return setmetatable({ size = size, nxt = nil }, Disk) end

local Towers = {}
Towers.__index = Towers
function Towers:pushDisk(disk, pile)
  local top = self.piles[pile]
  if top ~= nil and disk.size >= top.size then error("Cannot put a big disk on a smaller one") end
  disk.nxt = top
  self.piles[pile] = disk
end
function Towers:popDiskFrom(pile)
  local top = self.piles[pile]
  if top == nil then error("Attempting to remove a disk from an empty pile") end
  self.piles[pile] = top.nxt
  top.nxt = nil
  return top
end
function Towers:moveTopDisk(from, to)
  self:pushDisk(self:popDiskFrom(from), to)
  self.movesDone = self.movesDone + 1
end
function Towers:buildTowerAt(pile, disks)
  for i = disks, 0, -1 do self:pushDisk(newDisk(i), pile) end
end
function Towers:moveDisks(disks, from, to)
  if disks == 1 then
    self:moveTopDisk(from, to)
  else
    local other = 3 - from - to
    self:moveDisks(disks - 1, from, other)
    self:moveTopDisk(from, to)
    self:moveDisks(disks - 1, other, to)
  end
end

local function rounds()
  local total, ok = 0, 0
  for it = 1, N do
    local t = setmetatable({ piles = {}, movesDone = 0 }, Towers)
    t:buildTowerAt(1, 13)
    t:moveDisks(13, 1, 2)
    total = total + t.movesDone
    if t.movesDone == 8191 then ok = ok + 1 end
  end
  return ok, total
end
print("towers", rounds())

-- the piles table of the program is shared between rounds and sized differently
local function shared(n)
  local piles = { [0] = false, false, false }
  local s = 0
  for i = 1, n do
    local a, b = i % 3, (i + 1) % 3
    piles[a] = piles[a] or newDisk(i)
    local d = piles[a]
    piles[a] = nil
    piles[b] = piles[b] or d
    s = s + (piles[a] == nil and 1 or 2) + piles[b].size % 5
  end
  return s
end
print(shared(400))
"##,
    },
    Prog {
        name: "j2_dead",
        small: false,
        src: r##"
-- J2-1: a method-heavy loop with no recursion in it (the continuation heads of its calls are refused until a native self-call is
-- recorded, which never happens: the gate drops the heads), next to a real recursion (it keeps its traces).
local Obj = {}
Obj.__index = Obj
function Obj.new(x) return setmetatable({ x = x, n = 0 }, Obj) end
function Obj:step(k) self.n = self.n + k; return self.n end
function Obj:twice(k) return self:step(k) + self:step(k + 1) end
function Obj:thrice(k)
  local a = self:twice(k)
  local b = self:step(a % 7)
  return a + b + self:twice(2)
end
local function work(o, n)
  local s = 0
  for i = 1, n do s = s + o:thrice(i % 7) end
  return s
end
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local o = Obj.new(1)
local t = 0
for r = 1, 70 do t = t + work(o, 40) + fib(r % 13) end
print(t, o.n)
-- a second object, a second shape of the same calls
local P = setmetatable({}, { __index = Obj })
local p = setmetatable({ x = 2, n = 5 }, { __index = P })
local u = 0
for r = 1, 40 do u = u + work(p, 30) end
print(u, p.n)
"##,
    },
    Prog {
        name: "j2_deep",
        small: false,
        src: r##"
-- J2-2: recursion inside a hot loop (rec_loop): the loop recording stops at the recursion unroll limit; the site gets one deeper
-- re-record. Then a loop whose recursion is too deep for any limit (fib(10 + i % 4): the deep recording stops at the limit again),
-- and one whose recorded call tree is too big (the IR bound).
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
local function run(n)
  local s = 0
  for i = 1, n do s = s + fib(i % 3 + 4) + ack(1, i % 4) end
  return s
end
local total = 0
for r = 1, 60 do total = total + run(100) end
print(total)
local function deeper(n)
  local s = 0
  for i = 1, n do s = s + fib(10 + i % 4) end
  return s
end
local d = 0
for r = 1, 12 do d = d + deeper(60) end
print(d)
local function big(n, a, b, c)
  if n < 2 then return a + b + c end
  local x = big(n - 1, a + 1, b * 2 % 1009, c + n)
  local y = big(n - 2, b, c, a)
  local z = (x * 3 + y * 5 + a * 7 + b * 11 + c * 13) % 100003
  z = (z * z + x + y + a + b + c + n) % 100003
  z = (z * 31 + x * 17 + y * 19 + n * 23) % 100003
  return z
end
local function bigloop(n)
  local s = 0
  for i = 1, n do s = (s + big(i % 4 + 3, i, i + 1, i + 2)) % 1000003 end
  return s
end
local e = 0
for r = 1, 40 do e = (e + bigloop(60)) % 1000003 end
print(e)
"##,
    },
    Prog {
        name: "j2_gate_sides",
        small: false,
        src: r##"
-- J2-1 (budget rule): a recursive function with data-dependent paths (many side traces), a recursive method, and a loop around them
local function walk(t, d)
  if d == 0 then return 1 end
  local s = 0
  for i = 1, #t do
    local v = t[i]
    if v % 3 == 0 then s = s + walk(t, d - 1)
    elseif v % 3 == 1 then s = s + 2
    else s = s + (v > 20 and 3 or 4) end
  end
  return s
end
local tree = {}
for i = 1, 12 do tree[i] = (i * 7) % 29 end
local total = 0
for r = 1, 80 do
  total = total + walk(tree, 2 + r % 3)
  tree[r % 12 + 1] = (tree[r % 12 + 1] + r) % 31
end
print(total)
local Node = {}
Node.__index = Node
function Node:sum(d)
  if d == 0 then return self.v end
  local s = self.v
  if self.l then s = s + self.l:sum(d - 1) end
  if self.r then s = s + self.r:sum(d - 1) end
  return s
end
local function mk(d, v)
  if d == 0 then return setmetatable({ v = v }, Node) end
  return setmetatable({ v = v, l = mk(d - 1, v * 2), r = d % 2 == 0 and mk(d - 1, v * 2 + 1) or nil }, Node)
end
local root = mk(6, 1)
local acc = 0
for r = 1, 150 do acc = acc + root:sum(r % 7) end
print(acc)
"##,
    },
    Prog {
        name: "k_strself",
        small: true,
        src: r##"
-- BUNDLE-K (B): the tier-1 SELF on a string receiver probes the string metatable's `__index` table inline. Hot loops of method calls on
-- short and long strings and a polymorphic site, with the situation changed in the middle of the loop: string.* monkey-patched (replaced,
-- removed, nil-valued), the metatable's `__index` replaced by another table / a chain / a function / nil / a number, the string metatable
-- removed or swapped, a metatable on the `string` table itself.
local N = 200
local out = {}
local function emit(...) out[#out + 1] = table.concat({ ... }, ",") end
local S = string
local mt = getmetatable("")
local oidx = mt.__index
local osub, olen, oup = S.sub, S.len, S.upper
local obj = { sub = function(self, a) return "obj" .. a end, upper = function() return "OBJ" end, len = function() return 3 end }
local items = { "hello world", obj, ("xyz"):rep(30), "q" }
local function body(i)
  local s = items[i % 4 + 1]
  local a = s:sub(1, i % 5 + 1)
  local b = s:upper()
  local c = s:len()
  return #tostring(a) + #tostring(b) + c
end
local function loop(at, patch, unpatch)
  local acc, errs = 0, 0
  for i = 1, N do
    if i == at then patch() end
    local ok, r = pcall(body, i)
    if ok then acc = acc + r else errs = errs + 1 end
  end
  unpatch()
  return acc, errs
end
local function nop() end
emit(loop(0, nop, nop))
local alt = { sub = function() return "ALT" end, upper = function() return "ALTUP" end, len = function() return 1 end }
emit(loop(50, function() S.sub = function(s, i) return "P" .. i end end, function() S.sub = osub end))
emit(loop(50, function() S.len = function() return 42 end end, function() S.len = olen end))
emit(loop(50, function() S.upper = nil end, function() S.upper = oup end))
emit(loop(50, function() S.upper = nil; S.upper = function(s) return s .. "!" end end, function() S.upper = oup end))
emit(loop(50, function() mt.__index = alt end, function() mt.__index = oidx end))
emit(loop(50, function() mt.__index = setmetatable({}, { __index = S }) end, function() mt.__index = oidx end))
emit(loop(50, function() mt.__index = setmetatable({ sub = alt.sub }, { __index = function(t, k) return S[k] end }) end, function() mt.__index = oidx end))
emit(loop(50, function() mt.__index = function(s, k) return alt[k] or S[k] end end, function() mt.__index = oidx end))
emit(loop(50, function() mt.__index = nil end, function() mt.__index = oidx end))
emit(loop(50, function() mt.__index = 5 end, function() mt.__index = oidx end))
emit(loop(50, function() debug.setmetatable("", nil) end, function() debug.setmetatable("", mt) end))
emit(loop(50, function() debug.setmetatable("", { __index = alt }) end, function() debug.setmetatable("", mt) end))
emit(loop(50, function() setmetatable(S, { __index = function(t, k) return alt[k] end }); S.sub = nil end, function() setmetatable(S, nil); S.sub = osub end))
emit(loop(0, nop, nop))
-- a method that is absent (nil) and a number receiver
emit(select(2, pcall(function() return ("x"):nosuch() end)))
emit(select(2, pcall(function() local n = 5; return n:sub(1) end)))
print(table.concat(out, "\n"))
"##,
    },
    Prog {
        name: "l_d1",
        small: true,
        src: r##"
-- BUNDLE-L (D1): collectables stored into closed upvalues from a hot loop (upvalue barrier), loads of absent keys (integer keys outside the array
-- part, string keys, nil-valued nodes, keys that appear/disappear/rehash between calls, metatables with and without __index added in the middle)
-- and keys built by CONCAT. Allocation in every iteration keeps the collector busy.
local out = {}
local function emit(...)
  local t = { ... }
  for i = 1, select("#", ...) do t[i] = tostring(t[i]) end
  out[#out + 1] = table.concat(t, ",")
end
-- A: stores into a closed upvalue
local function mk()
  local last, cnt, fn, str, tbl = nil, 0, nil, "s", {}
  local function put(v) last = v; cnt = cnt + 1 end
  local function putf(f) fn = f end
  local function puts(s) str = s end
  local function putt(t) tbl = t end
  local function get() return last, cnt, fn, str, tbl end
  return put, putf, puts, putt, get
end
local function loopA(put, putf, puts, putt, get, n)
  local acc = 0
  for i = 1, n do
    -- the previous value (stored an iteration ago, referenced from nothing but the upvalue) is read back after allocations that would reuse its
    -- memory if the collector had freed it
    local p = get()
    if p then acc = acc + p[1] + p[2][1] end
    local g = { i, { i } }
    put(g)
    if i % 3 == 0 then putf(function() return i end) end
    puts("k" .. (i % 7))
    putt({ x = i, y = { z = i } })
    local j1, j2 = { i + 1, { i + 1 } }, { i + 2, { i + 2 } }
    acc = acc + #g + #j1 - #j2
  end
  local l, c, f, s, t = get()
  return l[1] + l[2][1], c, f and f() or -1, s, t.x + t.y.z, acc
end
for r = 1, 6 do
  local a, b, c, d, e = mk()
  emit("A", loopA(a, b, c, d, e, 20 + r * 11))
  collectgarbage()
end
-- a shared upvalue seen by two closures, stored from the loop, read in the loop
local function shared(n)
  local cell = nil
  local function set(v) cell = v end
  local function read() return cell end
  local s = 0
  for i = 1, n do
    set({ i })
    local c = read()
    s = s + c[1]
    local junk = { i, i + 1, i + 2 }
  end
  return s
end
emit("A2", shared(60), shared(61), shared(200))
-- B: absent integer keys. (Every scenario compiles its own copy of the loop: `load` makes a new prototype, so a trace is never shared.)
local LOADINT = [==[
return function(t, n, base)
  local c, nils = 0, 0
  for i = 1, n do
    local v = t[i * 3 + base]
    if v == nil then nils = nils + 1 else c = c + v end
  end
  return c, nils
end]==]
local function loader(src) return assert(load(src))() end
local loadInt = loader(LOADINT)
local sparse = {}
for k = 1, 40 do sparse[k * 9] = k end
sparse[-5] = 7; sparse[0] = 9
for r = 1, 12 do
  if r == 5 then for k = 1, 30 do sparse[k * 9 + 1] = k end end        -- rehash
  if r == 7 then for k = 1, 40 do sparse[k * 9] = nil end end           -- nil-valued / dead nodes
  if r == 8 then setmetatable(sparse, { __index = function(t, k) return 1 end }) end
  if r == 10 then setmetatable(sparse, { __index = { [14] = 100, [17] = 50 } }) end
  if r == 12 then setmetatable(sparse, {}); sparse[2] = 1; sparse[3] = 2 end
  emit("B", r, loadInt(sparse, 90, r % 3))
end
local loadGrow = loader(LOADINT)
local grow = {}
for r = 1, 10 do
  if r == 6 then for i = 1, 24 do grow[i] = i end end   -- the keys move into the array part after a rehash
  emit("B3", r, loadGrow(grow, 40, -2))
end
local loadArr = loader(LOADINT)
local arr = { 1, 2, 3 }
for r = 1, 12 do
  if r == 6 then arr[4] = 4; arr[5] = 5 end
  if r == 9 then arr[1000] = 1 end
  emit("B2", r, loadArr(arr, 60, 0), #arr)
end
-- C: absent string keys, loaded from a table of names
local names = {}
for i = 1, 24 do names[i] = "name" .. i end
local reg = {}
for i = 1, 24, 2 do reg[names[i]] = i end
local LOADSTR = [==[
local names = ...
return function(t, n)
  local c, nils = 0, 0
  for i = 1, n do
    local v = t[names[i % 24 + 1]]
    if v == nil then nils = nils + 1 else c = c + v end
  end
  return c, nils
end]==]
local loadStr = assert(load(LOADSTR))(names)
for r = 1, 14 do
  if r == 4 then for i = 2, 24, 2 do reg[names[i]] = -i end end        -- the missing keys appear
  if r == 6 then for i = 1, 24, 3 do reg[names[i]] = nil end end       -- tombstones
  if r == 8 then for i = 1, 200 do reg["extra" .. i] = i end end       -- rehash
  if r == 10 then setmetatable(reg, { __index = function(t, k) return #k end }) end
  if r == 12 then setmetatable(reg, { __index = { name2 = 1000 } }) end
  if r == 14 then setmetatable(reg, { __newindex = function() end }) end
  emit("C", r, loadStr(reg, 70))
end
-- D: keys built by CONCAT
local CONCATLOAD = [==[
return function(t, n)
  local c, nils = 0, 0
  for i = 1, n do
    local v = t["key" .. (i % 17)]
    if v == nil then nils = nils + 1 else c = c + v end
  end
  return c, nils
end]==]
local concatLoad = loader(CONCATLOAD)
local ck = {}
for r = 1, 12 do
  if r == 3 then for i = 0, 16, 2 do ck["key" .. i] = i end end
  if r == 5 then for i = 1, 16, 2 do ck["key" .. i] = 100 + i end end
  if r == 7 then for i = 0, 16, 4 do ck["key" .. i] = nil end end
  if r == 9 then setmetatable(ck, { __index = function(t, k) return 5 end }) end
  if r == 11 then setmetatable(ck, nil); for i = 1, 100 do ck["z" .. i] = i end end
  emit("D", r, concatLoad(ck, 60))
end
-- E: absent loads feeding stores (the load result is stored into another table and into an upvalue)
local function copyAbs(src, dst, n)
  local hold
  local function keep(v) hold = v end
  local c = 0
  for i = 1, n do
    local v = src[i + 1000] or { i }
    keep(v)
    dst[i] = v
    c = c + v[1]
  end
  return c, #dst
end
for r = 1, 4 do
  local src = {}
  if r == 3 then for i = 1, 30 do src[i + 1000] = { -i } end end
  emit("E", r, copyAbs(src, {}, 50))
end
print(table.concat(out, "\n"))
"##,
    },
    Prog {
        name: "m_nyi_closure",
        small: true,
        src: r##"
-- BUNDLE-M C4: a loop with a CLOSURE in its body aborts `Nyi(Closure)` at every recording (a property of the code)
local function f(n)
  local s = 0
  for i = 1, n do
    local h = function() return i end
    s = s + h()
  end
  return s
end
local r = 0
for k = 1, 80 do r = r + f(30) end
print(r)
"##,
    },
    Prog {
        name: "m_inner_loop",
        small: true,
        src: r##"
-- BUNDLE-M C4: a loop whose body holds a `while` loop with a data-dependent trip count: the recorded path reaches an inner loop head
local function f(n)
  local s = 0
  local i = 0
  while i < n do
    local j = 0
    while j < i % 3 do j = j + 1 s = s + j end
    i = i + 1
    s = s + 1
  end
  return s
end
local r = 0
for k = 1, 120 do r = r + f(40) end
print(r)
"##,
    },
    Prog {
        name: "m_probe_native",
        small: true,
        src: r##"
-- BUNDLE-M C2: a function-entry head whose path calls a C function the recorder has no recording for BEFORE the self-call: every recording ends
-- `FnShort` at that call (the probe must stop there, not walk on to the recursion)
local function g(n)
  if n <= 0 then return 0 end
  local c = os.getenv("RUSTYNAIL_NOPE")
  return n + g(n - 1)
end
local function drive(n)
  local s = 0
  for i = 1, n do s = s + g(3) end
  return s
end
local r = 0
for k = 1, 60 do r = r + drive(30) end
print(r)
"##,
    },
    Prog {
        name: "m_probe_chain",
        small: true,
        src: r##"
-- BUNDLE-M C2: a call cycle longer than the inlining depth (a -> b -> c -> d -> e -> f -> h, b -> a): the recording ends `FnShort` at the depth
-- limit before it reaches the self-call (the probe must apply the same limit)
local a, b, c, d, e, f, h
function h(n) return n + 1 end
function f(n) return h(n) + 1 end
function e(n) return f(n) + 1 end
function d(n) return e(n) + 1 end
function c(n) return d(n) + 1 end
function b(n) if n <= 0 then return 0 end return c(n) + a(n - 1) end
function a(n) if n <= 0 then return 0 end return b(n) + 1 end
local function drive(n)
  local s = 0
  for i = 1, n do s = s + a(3) end
  return s
end
local r = 0
for k = 1, 60 do r = r + drive(30) end
print(r)
"##,
    },
    Prog {
        name: "m_probe_loops",
        small: true,
        src: r##"
-- BUNDLE-M C2: a function-entry head of native recursion whose callee has a loop: with a short trip count the recording unrolls it and ends
-- `FnShort` at the root RETURN, with a long one it ends `InnerLoop` (the probe must apply the same unroll limit)
local function loopy(k)
  local s = 0
  for i = 1, k do s = s + i end
  return s
end
local function g(n, k)
  if n <= 0 then return loopy(k) end
  return n + g(n - 1, k)
end
local function drive(n, d, k)
  local s = 0
  for i = 1, n do s = s + g(d, k) end
  return s
end
local r = 0
for q = 1, 40 do r = r + drive(30, 0, 2) end
for q = 1, 40 do r = r + drive(30, 0, 50) end
print(r)
"##,
    },
    Prog {
        name: "dm_dynkey",
        small: true,
        src: r##"
-- DYNMASK: dynamic short-string keys with the node count / node vector read at run time (no lsizenode guard), nil-miss loads, NEWREF for
-- absent dynamic stores. A table that rehashes between lookups of one trace, __index / __newindex chains, long strings, float keys,
-- nil / NaN keys, array parts, dead keys, collector pressure.
local out = {}
local function emit(...)
  local t = { ... }
  for i = 1, select("#", ...) do t[i] = tostring(t[i]) end
  out[#out + 1] = table.concat(t, ",")
end
local function loader(src, ...) return assert(load(src))(...) end
local names = {}
for i = 1, 300 do names[i] = "k" .. i end

-- A: lookup then insert in the same loop: the table rehashes under the trace (insert in loop, then lookup)
local GROW = [==[
local names = ...
return function(t, n, off)
  local hits, misses, s = 0, 0, 0
  for i = 1, n do
    local k = names[(i + off) % 300 + 1]
    local v = t[k]
    if v then hits = hits + 1; s = s + v else misses = misses + 1; t[k] = i end
  end
  return hits, misses, s
end]==]
local grow = loader(GROW, names)
for r = 1, 6 do
  local t = {}
  emit("A", r, grow(t, 120 * r, r), grow(t, 400, 3 * r))
end
local persistent = {}
for r = 1, 8 do emit("A2", r, grow(persistent, 50 + r * 40, r * 7)) end

-- A3: the same table is read, inserted into and read again (two lookups of one key around the insertion), counts per key
local COUNT = [==[
local names = ...
return function(t, n, m)
  local c = 0
  for i = 1, n do
    local k = names[i % m + 1]
    t[k] = (t[k] or 0) + 1
    c = c + t[k]
  end
  return c
end]==]
local count = loader(COUNT, names)
for r = 1, 8 do
  local t = {}
  emit("A3", r, count(t, 200, 20 + r * 30), count(t, 100, 300))
  local s = 0
  for _, k in ipairs(names) do s = s + (t[k] or 0) end
  emit("A3s", s)
end

-- B: tables of different sizes in one trace (the node count differs between iterations), including the empty table (dummy node)
local MULTI = [==[
local names = ...
return function(tabs, n)
  local s, nils = 0, 0
  for i = 1, n do
    local t = tabs[i % #tabs + 1]
    local v = t[names[i % 40 + 1]]
    if v then s = s + v else nils = nils + 1 end
  end
  return s, nils
end]==]
local multi = loader(MULTI, names)
local tabs = { {}, { k1 = 1 }, { k1 = 1, k2 = 2 }, {} }
local big = {}
for i = 1, 64 do big[names[i]] = i end
tabs[#tabs + 1] = big
local mid = {}
for i = 1, 5 do mid[names[i * 2]] = -i end
tabs[#tabs + 1] = mid
for r = 1, 10 do
  if r == 5 then for i = 1, 200 do tabs[1][names[i]] = i end end     -- the empty table grows
  if r == 7 then for i = 1, 64 do big[names[i]] = nil end end         -- dead keys
  if r == 9 then for i = 1, 30 do big[names[i + 3]] = i end end
  emit("B", r, multi(tabs, 120))
end

-- C: __index chains (a local miss, then the parent): one trace, several keys
local parent = {}
for i = 1, 16 do parent[names[i]] = i end
local env = setmetatable({}, { __index = parent })
local CHAIN = [==[
local names = ...
return function(env, n, m)
  local s, nils = 0, 0
  for i = 1, n do
    local v = env[names[i % m + 1]]
    if v == nil then nils = nils + 1 else s = s + v end
  end
  return s, nils
end]==]
local chain = loader(CHAIN, names)
for r = 1, 16 do
  if r == 4 then for i = 1, 16, 3 do env[names[i]] = 100 + i end end                  -- some keys shadowed
  if r == 6 then for i = 1, 16, 3 do env[names[i]] = nil end end                      -- nil-valued nodes
  if r == 8 then for i = 17, 40 do env[names[i]] = i end end                          -- rehash of env
  if r == 10 then setmetatable(parent, { __index = function(t, k) return #k end }) end
  if r == 12 then getmetatable(env).__index = function(t, k) return 7 end end
  if r == 14 then getmetatable(env).__index = parent end
  if r == 15 then setmetatable(env, nil) end
  emit("C", r, chain(env, 90, 24))
end
-- three levels
local g1, g2 = { k1 = 1, k5 = 5 }, { k2 = 2 }
local g3 = setmetatable({ k3 = 3 }, { __index = g2 })
setmetatable(g2, { __index = g1 })
for r = 1, 10 do
  if r == 5 then g1.k9 = 9; g2.k8 = 8 end
  if r == 7 then g3.k1 = 100 end
  emit("C3", r, chain(g3, 80, 12))
end

-- D: rawget with dynamic keys (the metatable is never looked at)
local RAW = [==[
local names = ...
return function(t, n)
  local s, nils = 0, 0
  for i = 1, n do
    local v = rawget(t, names[i % 30 + 1])
    if v == nil then nils = nils + 1 else s = s + v end
  end
  return s, nils
end]==]
local raw = loader(RAW, names)
local rt = setmetatable({}, { __index = function() return 1000 end })
for i = 1, 30, 2 do rt[names[i]] = i end
for r = 1, 8 do
  if r == 4 then for i = 2, 30, 2 do rt[names[i]] = -i end end
  if r == 6 then for i = 1, 60 do rt[names[i + 100]] = i end end
  emit("D", r, raw(rt, 90))
end

-- E: stores through __newindex (function and table), present / absent / nil-valued keys
local logs = 0
local sink = {}
local nt = setmetatable({}, { __newindex = function(t, k, v) logs = logs + 1; rawset(t, k, v) end })
local nt2 = setmetatable({}, { __newindex = sink })
local STORE = [==[
local names = ...
return function(t, n, m)
  for i = 1, n do t[names[i % m + 1]] = i end
end]==]
local store = loader(STORE, names)
for r = 1, 6 do
  store(nt, 70, 10 + r * 3)
  store(nt2, 70, 10 + r * 3)
  if r == 3 then for i = 1, 40 do nt[names[i]] = nil end end
  local s1, s2 = 0, 0
  for i = 1, 60 do s1 = s1 + (rawget(nt, names[i]) or 0); s2 = s2 + (sink[names[i]] or 0) + (rawget(nt2, names[i]) or 0) end
  emit("E", r, logs, s1, s2)
end

-- F: key kinds: long strings, integer-valued floats, strings that look like numbers, mixed in one loop (the type guard of the key exits)
local keys = {}
for i = 1, 6 do keys[#keys + 1] = names[i] end
keys[#keys + 1] = string.rep("L", 60) .. "a"
keys[#keys + 1] = string.rep("L", 60) .. "b"
keys[#keys + 1] = 1.0
keys[#keys + 1] = 2
keys[#keys + 1] = "1"
keys[#keys + 1] = 2.5
keys[#keys + 1] = true
local KINDS = [==[
local keys = ...
return function(t, n)
  local s = 0
  for i = 1, n do
    local k = keys[i % #keys + 1]
    t[k] = (t[k] or 0) + i
    s = s + t[k]
  end
  return s
end]==]
local kinds = loader(KINDS, keys)
for r = 1, 8 do
  local t = { 10, 20, 30 }
  t[keys[1]] = 1; t[keys[7]] = 7
  local total = kinds(t, 130 + r)
  emit("F", r, total, t[1], t[2], t[3], t["1"], t[2.5], t[true], t[keys[7]], t[keys[8]])
end
-- only short strings for a long time, then a long string comes in
local MIXED = [==[
local keys, tbl = ...
return function(n, flip)
  local s = 0
  for i = 1, n do
    local k = keys[i % 8 + 1]
    if flip and i > 90 then k = keys[9] end
    s = s + (tbl[k] or 0)
  end
  return s
end]==]
local mk = {}
for i = 1, 8 do mk[i] = names[i] end
mk[9] = string.rep("Z", 50)
local mt = {}
for i = 1, 8 do mt[names[i]] = i end
mt[mk[9]] = 1000
local mixed = loader(MIXED, mk, mt)
for r = 1, 8 do emit("F2", r, mixed(120, r >= 5)) end

-- G: nil / NaN keys: loads give nil, stores raise the same errors
local BADKEY = [==[
local names = ...
return function(t, n, bad)
  local s = 0
  for i = 1, n do
    local k = names[i % 5 + 1]
    if i == n then k = bad end
    local ok, e = pcall(function() t[k] = i end)
    if not ok then return "err", e end
    s = s + (t[k] or 0)
  end
  return s
end]==]
local badkey = loader(BADKEY, names)
for r = 1, 4 do
  local t = {}
  emit("G", r, badkey(t, 60, nil))
  emit("G", r, badkey(t, 60, 0 / 0))
  emit("G", r, badkey(t, 60, "ok"))
  local ok, e = pcall(function() return t[nil], t[0 / 0] end)
  emit("G2", r, ok, e)
end

-- H: tables with an array part and a hash part, string keys that are decimal numbers, mixed with integer keys of the same table
local ARR = [==[
local names = ...
return function(t, n)
  local s = 0
  for i = 1, n do
    local k = names[i % 12 + 1]
    t[k] = (t[k] or 0) + 1
    s = s + t[k] + (t[i % 20 + 1] or 0)
    if i % 17 == 0 then t[#t + 1] = i end
  end
  return s, #t
end]==]
local arr = loader(ARR, names)
for r = 1, 8 do
  local t = {}
  for i = 1, r * 4 do t[i] = i end
  emit("H", r, arr(t, 100 + r))
end

-- I: dead keys: a key removed (set to nil), the collector runs, the key is stored again from a fresh table of keys
local DEAD = [==[
local names = ...
return function(t, n, kill)
  local s = 0
  for i = 1, n do
    local k = names[i % 25 + 1]
    if kill and i % 7 == 0 then t[k] = nil end
    local v = t[k]
    if v then s = s + v end
    t[k] = i
  end
  return s
end]==]
local dead = loader(DEAD, names)
local dt = {}
for r = 1, 12 do
  emit("I", r, dead(dt, 100, r % 2 == 0))
  if r % 3 == 0 then collectgarbage() end
  if r == 6 then dt = {} end
end

-- J: keys built by concatenation and by string library calls (fresh strings every iteration, interned short strings)
local CAT = [==[
return function(t, n)
  local s = 0
  for i = 1, n do
    local k = "k" .. (i % 40 + 1)
    local v = t[k]
    if v then s = s + v else t[k] = i end
    local k2 = string.sub("xk" .. (i % 13 + 1), 2)
    s = s + (t[k2] or 0)
  end
  return s
end]==]
local cat = loader(CAT)
for r = 1, 8 do
  local t = {}
  emit("J", r, cat(t, 60 + r * 20), cat(t, 100))
end

-- K: stores through computed keys into nil-valued (present) nodes, then into keys the table does not have at all (a miss must exit, never
-- write): the nodes of the first keys were emptied without a rehash
local NILST = [==[
local names = ...
return function(t, n, m)
  for i = 1, n do t[names[i % m + 1]] = i end
end]==]
local nilst = loader(NILST, names)
for r = 1, 8 do
  local t = {}
  for i = 1, 20 do t[names[i]] = 1 end
  for i = 1, 20 do t[names[i]] = nil end
  nilst(t, 20, 20)
  nilst(t, 60 + r, 40)
  local s = 0
  for i = 1, 40 do s = s + (t[names[i]] or 0) end
  emit("K", r, s)
end
print(table.concat(out, "\n"))
"##,
    },
    Prog {
        name: "dm_growth",
        small: false,
        src: r##"
-- DYNMASK: a table that grows (rehashes) between the lookups of one loop: with the run-time node count there is no exit per rehash. The
-- counters are the check (the same numbers in every mode); the exit count is bounded by the test.
local keys = {}
for i = 1, 1500 do keys[i] = "g" .. i end
local t, hits, adds = {}, 0, 0
for i = 1, 5000 do
  local k = keys[i % 1500 + 1]
  local v = t[k]
  if v then hits = hits + 1; t[k] = v + 1 else adds = adds + 1; t[k] = 1 end
end
local u, miss = {}, 0
for i = 1, 4000 do
  local k = keys[i % 1000 + 1]
  if u[k] == nil then miss = miss + 1; u[k] = i end
end
local n, tot = 0, 0
for _, v in pairs(t) do n = n + 1; tot = tot + v end
print(hits, adds, miss, n, tot, t[keys[1]], t[keys[1500]], u[keys[1000]])
"##,
    },
    Prog {
        name: "t1i_inline",
        small: true,
        src: r##"
-- BUNDLE-T1I: inline built-ins in the tier-1 CALL template: type, math.floor/abs/max/min/sqrt, string.byte/len, select('#', ...), rawlen.
-- Every built-in is called from hot functions (so that tier 1 compiles the call sites) with normal and abnormal arguments (the generic path must
-- produce the identical result or the identical error message), then the situation changes in the middle of a hot loop: the global or field is
-- replaced (and restored), a hook is installed, the call is made through a local alias, with other argument counts and through multiple results.
local N = 260
local out = {}
local H_type, H_tostring, H_mtype, H_format, H_select, H_concat, H_gsub = type, tostring, math.type, string.format, select, table.concat, string.gsub
local function emit(...)
  local t = {}
  for i = 1, H_select("#", ...) do t[i] = H_tostring((H_select(i, ...))) end
  out[#out + 1] = H_concat(t, ",")
end
local function fmt(v)
  local ty = H_mtype(v)
  if ty == "float" then
    if v ~= v then return "nan" end
    return H_format("%a", v)
  elseif ty == "integer" then
    return H_tostring(v) .. "i"
  end
  if H_type(v) == "string" then return (H_gsub(H_format("%q", v), "0x%x+", "0xADDR")) end
  local s = H_gsub(H_tostring(v), "0x%x+", "0xADDR")
  return s
end
local function cat(ok, ...)
  local t = { H_tostring(ok) }
  for i = 1, H_select("#", ...) do t[#t + 1] = fmt((H_select(i, ...))) end
  return H_concat(t, " ")
end

-- the sites
local function f_type(v) local r = type(v) return r end
local function f_type0(v) local r = tostring(type(v)) return r end
local function f_typem(v) local r = { type(v) } return #r, r[1] end
local function f_type2(a, b) local r = type(a, b) return r end
local function f_type_none() local r = type() return r end
local function f_floor(v) local r = math.floor(v) return r end
local function f_abs(v) local r = math.abs(v) return r end
local function f_sqrt(v) local r = math.sqrt(v) return r end
local function f_max(a, b) local r = math.max(a, b) return r end
local function f_min(a, b) local r = math.min(a, b) return r end
local function f_max1(a) local r = math.max(a) return r end
local function f_max3(a, b, c) local r = math.max(a, b, c) return r end
local function f_byte1(s) local r = string.byte(s) return r end
local function f_byte2(s, i) local r = string.byte(s, i) return r end
local function f_byte3(s, i, j) local r = string.byte(s, i, j) return r end
local function f_bytem(s, i) local r = { string.byte(s, i) } return #r, r[1] end
local function f_len(s) local r = string.len(s) return r end
local function f_lenm(s) local r = s:len() return r end
local function f_rawlen(v) local r = rawlen(v) return r end
local function f_sel(...) local r = select("#", ...) return r end
local function f_sel1(...) local r = select("#") return r end
local function f_sel3(a, b, c) local r = select("#", a, b, c) return r end
local function f_selm(...) local r = { select("#", ...) } return #r, r[1] end
local function f_seln(n, ...) local r = { select(n, ...) } return #r, r[1] end
local function f_selbad(...) local r = select("x", ...) return r end

local big = string.rep("long string ", 6) -- 66 bytes: a long string
local vals = {
  0, -0.0, 0.0, 1, -1, 2, 3, 100, 1.5, -1.5, 0.5, -0.5, 2 ^ 53, -2 ^ 53, 2 ^ 63, -2 ^ 63, 2 ^ 62, 1e300, -1e300, 1e15 + 0.5,
  math.huge, -math.huge, 0 / 0, math.maxinteger, math.mininteger, math.maxinteger + 0.0, math.mininteger + 0.0, 4503599627370496.5,
  "10", "0x10", "abc", " 5 ", "", true, false, {}, print, f_type, type, nil, io.stdout, coroutine.create(f_type),
  string.byte, math.floor, setmetatable({}, { __index = function() return 1 end }),
}
local nvals = 47
local strs = { "", "a", "hello", "\0\255x", ("x"):rep(40), ("y"):rep(41), big, "10", 5, 1.5, true, nil, {}, 2 ^ 63 }
local idxs = { 1, 2, 3, 5, 0, -1, -2, -5, 40, 41, 66, 67, 1.0, 2.0, 1.5, "2", "x", math.maxinteger, math.mininteger, 2 ^ 63, true, {}, false }
local tabs

local function run1(name, f)
  local res = {}
  for i = 1, N do
    local k = i % nvals + 1
    local r = cat(pcall(f, vals[k]))
    if i <= nvals then res[k] = r end
  end
  emit(name, #res)
  for k = 1, nvals do emit(" ", k, res[k]) end
end
local function run2(name, f, as, bs)
  local res = {}
  for i = 1, N do
    local k, l = i % #as + 1, (i * 7 // 3) % #bs + 1
    local r = cat(pcall(f, as[k], bs[l]))
    if i <= #as * 5 then res[k * 100 + l] = r end
  end
  local keys = {}
  for k in pairs(res) do keys[#keys + 1] = k end
  table.sort(keys)
  emit(name, #keys)
  for _, k in ipairs(keys) do emit(" ", k, res[k]) end
end

run1("type", f_type)
run1("type0", f_type0)
run1("typem", f_typem)
run1("floor", f_floor)
run1("abs", f_abs)
run1("sqrt", f_sqrt)
run1("len", f_len)
run1("rawlen_vals", f_rawlen)
run2("max", f_max, vals, vals)
run2("min", f_min, vals, vals)
run2("type2", f_type2, vals, vals)
run2("max_mixed", f_max, { 1, 2.5, -0.0, 0.0, 0 / 0, 3, 3.0 }, { 2, 2.5, 0.0, -0.0, 1, 3.0, 3 })
run2("min_mixed", f_min, { 1, 2.5, -0.0, 0.0, 0 / 0, 3, 3.0 }, { 2, 2.5, 0.0, -0.0, 1, 3.0, 3 })
run2("max_f", f_max, { 1.5, -0.0, 0.0, 0 / 0, math.huge }, { 2.5, 0.0, -0.0, 1.0, 0 / 0 })
run2("min_f", f_min, { 1.5, -0.0, 0.0, 0 / 0, math.huge }, { 2.5, 0.0, -0.0, 1.0, 0 / 0 })
run2("max_i", f_max, { math.mininteger, math.maxinteger, 0, -1 }, { math.maxinteger, math.mininteger, -1, 0 })
run2("min_i", f_min, { math.mininteger, math.maxinteger, 0, -1 }, { math.maxinteger, math.mininteger, -1, 0 })
run2("byte2", f_byte2, strs, idxs)
run2("bytem", f_bytem, strs, idxs)
run1("byte1", f_byte1)
for _, s in ipairs(strs) do emit("byte1s", cat(pcall(f_byte1, s))) end
for i = 1, N do f_byte1("abc") f_byte2("abc", 2) end
emit("byte3", cat(pcall(f_byte3, "hello", 2, 4)), cat(pcall(f_byte3, "hello", -3, -1)), cat(pcall(f_byte3, "hello")))
emit("max_n", cat(pcall(f_max1, 3)), cat(pcall(f_max1, "x")), cat(pcall(f_max1)), cat(pcall(f_max3, 1, 5, 3)), cat(pcall(f_max3, 1.5, 5, 3)))
emit("type_n", cat(pcall(f_type_none)))

-- select('#', ...) with 0..many varargs (many: more than the stack's initial room)
for _, n in ipairs({ 0, 1, 2, 3, 5, 10, 19, 20, 21, 40, 100, 250 }) do
  local t = {}
  for i = 1, n do t[i] = i end
  local last
  for i = 1, 40 do last = cat(pcall(f_sel, table.unpack(t, 1, n))) end
  emit("sel", n, last, cat(pcall(f_selm, table.unpack(t, 1, n))))
end
emit("sel_fixed", cat(pcall(f_sel1)), cat(pcall(f_sel3, 1, 2, 3)), cat(pcall(f_sel3)))
for i = 1, N do f_sel3(i, nil, nil) f_sel1() end
emit("seln", cat(pcall(f_seln, 2, "a", "b", "c")), cat(pcall(f_seln, -1, "a", "b", "c")), cat(pcall(f_seln, 0, "a")), cat(pcall(f_seln, 5, "a")),
  cat(pcall(f_seln, "#", "a", "b")), cat(pcall(f_seln, "#x", "a", "b")), cat(pcall(f_seln, "", "a")), cat(pcall(f_selbad, 1)), cat(pcall(f_selbad)))
for i = 1, N do pcall(f_selbad, "#", i) end

-- rawlen of tables: hint states
do
  local t = {}
  local res = {}
  for i = 1, 64 do
    t[#t + 1] = i
    res[#res + 1] = f_rawlen(t)
    if i % 3 == 0 then res[#res + 1] = f_rawlen(t) end
  end
  for i = 64, 1, -1 do
    t[i] = nil
    res[#res + 1] = f_rawlen(t)
  end
  emit("rawlen_grow", table.concat(res, " "))
  local fixed = { 1, 2, 3, 4, 5, 6, 7, 8 }
  for i = 1, N do res[i % 8 + 1] = f_rawlen(fixed) end
  fixed[8] = nil
  emit("rawlen_hole", f_rawlen(fixed), f_rawlen({ 1, 2, nil, 4 }), f_rawlen({ n = 1 }), f_rawlen({}), f_rawlen(setmetatable({}, { __len = function() return 99 end })))
  local sparse = { 1, 2, 3, nil, 5, nil, nil, 8 }
  local r2 = {}
  for i = 1, N do r2[#r2 + 1] = f_rawlen(sparse); sparse[(i % 8) + 1] = (i % 3 == 0) and nil or i end
  emit("rawlen_sparse", table.concat(r2, " ", 1, 60))
  local h = {}
  for i = 1, 20 do h[i] = i end
  for i = 20, 11, -1 do h[i] = nil; emit("rawlen_shrink", f_rawlen(h), #h, f_rawlen(h)) end
  emit("rawlen_str", f_rawlen("abc"), f_rawlen(""), f_rawlen(big), cat(pcall(f_rawlen, 5)), cat(pcall(f_rawlen)))
end

-- the situation changes in the middle of a hot loop
local function swap_loop(name, body, patch, unpatch)
  local acc = {}
  for i = 1, N do
    if i == 90 then patch() end
    if i == 180 then unpatch() end
    acc[#acc + 1] = cat(pcall(body, i))
  end
  local h = 0
  for i, v in ipairs(acc) do h = (h * 31 + #v + (i % 7)) % 1000003 end
  emit(name, h, acc[1], acc[100], acc[200], acc[N])
end
local omath = math
local otype, ofloor, oabs, omax, osqrt, obyte, olen, orawlen, oselect = type, math.floor, math.abs, math.max, math.sqrt, string.byte, string.len, rawlen, select
local function b_type(i) local r = type(i) return r, type(i + 0.5), type("s"), type({}) end
local function b_floor(i) local r = math.floor(i + 0.5) return r, math.floor(i) end
local function b_abs(i) return math.abs(-i), math.abs(i - 100.5) end
local function b_max(i) return math.max(i, 100), math.min(i, 100), math.max(i + 0.5, 100.5) end
local function b_sqrt(i) return math.sqrt(i), math.sqrt(i * i) end
local function b_byte(i) return string.byte("hello", i % 6), ("abc"):byte(i % 4) end
local function b_len(i) return string.len("hello"), string.len(("x"):rep(i % 50)) end
local function b_rawlen(i) return rawlen({ 1, 2, 3 }), rawlen("abcd") end
local function b_sel(i) local function g(...) return select("#", ...) end return g(), g(i), g(i, i) end
swap_loop("swap_type_nil", b_type, function() type = nil end, function() type = otype end)
swap_loop("swap_type_fn", b_type, function() type = function(x) return "T" end end, function() type = otype end)
swap_loop("swap_type_c", b_type, function() type = math.random end, function() type = otype end)
swap_loop("swap_type_cl", b_type, function() type = coroutine.wrap(function(...) while true do coroutine.yield("co") end end) end, function() type = otype end)
swap_loop("swap_type_print", b_type, function() type = tostring end, function() type = otype end)
swap_loop("swap_floor", b_floor, function() math.floor = function(x) return -7 end end, function() math.floor = ofloor end)
swap_loop("swap_floor_ceil", b_floor, function() math.floor = math.ceil end, function() math.floor = ofloor end)
swap_loop("swap_floor_nil", b_floor, function() math.floor = nil end, function() math.floor = ofloor end)
swap_loop("swap_math_tbl", b_floor, function() math = setmetatable({}, { __index = function(_, k) return function() return k end end }) end, function() math = omath end)
swap_loop("swap_abs", b_abs, function() math.abs = function(x) return 0 end end, function() math.abs = oabs end)
swap_loop("swap_max", b_max, function() math.max = math.min end, function() math.max = omax end)
swap_loop("swap_min", b_max, function() math.min = math.max end, function() math.min = math.min end)
swap_loop("swap_sqrt", b_sqrt, function() math.sqrt = math.abs end, function() math.sqrt = osqrt end)
swap_loop("swap_byte", b_byte, function() string.byte = string.len end, function() string.byte = obyte end)
swap_loop("swap_byte_fn", b_byte, function() string.byte = function() return "B" end end, function() string.byte = obyte end)
swap_loop("swap_len", b_len, function() string.len = function() return -1 end end, function() string.len = olen end)
swap_loop("swap_rawlen", b_rawlen, function() rawlen = function() return "RL" end end, function() rawlen = orawlen end)
swap_loop("swap_rawlen_len", b_rawlen, function() rawlen = string.len end, function() rawlen = orawlen end)
swap_loop("swap_select", b_sel, function() select = function() return "SEL" end end, function() select = oselect end)
swap_loop("swap_select_rl", b_sel, function() select = rawlen end, function() select = oselect end)
swap_loop("swap_strmt", b_byte, function() getmetatable("").__index = { byte = function() return "M" end } end, function() getmetatable("").__index = string end)

-- local aliases keep the identity after the global is replaced; a different function of the same signature does not
do
  local ltype, lfloor, lbyte, lmax = type, math.floor, string.byte, math.max
  local function al(i) return ltype(i), lfloor(i + 0.5), lbyte("abc", 2), lmax(i, 3) end
  local acc = {}
  for i = 1, N do
    if i == 100 then type = function() return "X" end; math.floor = function() return 0 end end
    acc[#acc + 1] = cat(pcall(al, i))
  end
  type, math.floor = otype, ofloor
  emit("alias", acc[1], acc[99], acc[101], acc[N])
  -- the alias itself changes
  local ftype, fbyte = otype, obyte
  local function al2(i) return ftype(i), fbyte("abc", 2) end
  local acc2 = {}
  for i = 1, N do
    if i == 100 then ftype = tostring; fbyte = math.max end
    acc2[#acc2 + 1] = cat(pcall(al2, i))
  end
  emit("alias2", acc2[1], acc2[99], acc2[101], acc2[N])
end

-- hooks: call, line and count hooks must see every call
do
  local function hooked(mask, count)
    local hits = 0
    local function hk() hits = hits + 1 end
    local function body(i) local a = type(i) local b = math.floor(i + 0.5) local c = string.byte("abc", 2) local d = select("#", i, i) return a, b, c, d end
    for i = 1, N do
      if i == 80 then debug.sethook(hk, mask, count or 0) end
      if i == 160 then debug.sethook() end
      body(i)
    end
    debug.sethook()
    return hits
  end
  emit("hook_call", hooked("c"))
  emit("hook_line", hooked("l"))
  emit("hook_count", hooked("", 3))
  emit("hook_ret", hooked("r"))
  -- a hook installed by a callee (a finalizer-like path): from inside a called function
  local hits = 0
  local function install() debug.sethook(function() hits = hits + 1 end, "c") end
  local function body2(i) local a = type(i) if i == 120 then install() end local b = type(a) return b end
  for i = 1, N do body2(i) end
  debug.sethook()
  emit("hook_install", hits > 0)
end

-- c == 0 / multiple wanted results / varargs consumers / the stack slots above
do
  local function m1(v) return type(v), type(v) end
  local function m2(v) local a, b = type(v) return a, b end
  local function m3(v) local a, b, c = math.floor(v), math.abs(v) return a, b, c end
  local function m4(v) return (type(v)) end
  local function m5(v) return type(v) end
  local function m6(v) return math.max(v, 1), string.len("abc") end
  local function m7(...) return select("#", ...), select("#", ...) end
  local function m8(v) local t = { type(v), math.floor(v), string.byte("x", 1) } return #t, t[1], t[2], t[3] end
  local function m9(v) return string.format("%s %s", type(v), math.floor(v)) end
  local acc = {}
  for i = 1, N do
    acc[#acc + 1] = cat(pcall(m1, i)) .. "|" .. cat(pcall(m2, i)) .. "|" .. cat(pcall(m3, i)) .. "|" .. cat(pcall(m4, i)) .. "|" .. cat(pcall(m5, i))
      .. "|" .. cat(pcall(m6, i)) .. "|" .. cat(pcall(m7, i, i, i)) .. "|" .. cat(pcall(m8, i + 0.0)) .. "|" .. cat(pcall(m9, i))
  end
  emit("multi", acc[1], acc[N])
  -- values above the results: getlocal of a frame sees the same locals (here: a deep expression keeps temporaries alive across calls)
  local function deep(v) return (math.max(math.floor(v), math.abs(-v)) + #type(v) + string.byte("a") + select("#", v, v, v)) end
  local s = 0
  for i = 1, N do s = s + deep(i) end
  emit("deep", s)
end

-- error text and position of the argument errors (name resolution through the calling instruction)
do
  local msgs = {}
  local function e1() return (math.floor("x")) end
  local function e2() local t = math.floor({}) return t end
  local function e3() local t = string.len(nil) return t end
  local function e4() local t = rawlen(5) return t end
  local function e5() local t = math.sqrt("x") return t end
  local function e6() local t = math.max(1, "x") return t end
  local function e7() local t = string.byte() return t end
  local function e8() local t = math.abs(nil) return t end
  local function e9() local t = select("#", nil) return t end
  local function e10() local m = math local t = m.floor(nil) return t end
  local function e11() local t = math.min({}, 1) return t end
  local function e12() local t = ("x"):byte({}) return t end
  for k, f in ipairs({ e1, e2, e3, e4, e5, e6, e7, e8, e9, e10, e11, e12 }) do
    local last
    for i = 1, 60 do last = cat(pcall(f)) end
    emit("err" .. k, last)
  end
end
-- mixing: one call site sees many callees (polymorphic), including Lua functions and natives that are not inlined
do
  local fns = { type, math.floor, math.abs, string.len, rawlen, tostring, function(x) return x end, math.sqrt, string.upper, select, next, math.type }
  local function poly(type, x) local r = type(x) return r end
  local acc = {}
  for i = 1, N * 2 do
    local k = i % #fns + 1
    local r = cat(pcall(poly, fns[k], (i % 3 == 0) and "ab" or i))
    if i <= #fns * 4 then acc[#acc + 1] = r end
  end
  emit("poly", table.concat(acc, ";"))
end
print(table.concat(out, "\n"))
"##,
    },
    Prog {
        name: "c36_ci",
        small: true,
        src: r##"
-- C36 (`cispare`): a hot loop with an inlined built-in (`math.floor`, `t1inline` bit 1) in a leaf that runs in fresh coroutines. A new coroutine has no spare
-- CallInfo nodes behind its frames (only a real call to a C function would allocate one, and tier 1 with the inline `floor` makes none), so every entry of the
-- trace (which needs one for the intrinsic) fails its TR6b check until something extends the chain. Without `cispare` the failure threshold flushes the root
-- (the re-record compiles to the same code and the head is blacklisted `Identical`); with it the chain is extended and the trace stays.
local BASE = 10000
local function mul_small(a, m)
  local r, carry = {}, 0
  for i = 1, #a do
    local p = a[i] * m + carry
    r[i] = p % BASE
    carry = math.floor(p / BASE)
  end
  while carry > 0 do
    r[#r + 1] = carry % BASE
    carry = math.floor(carry / BASE)
  end
  return r
end
local function work(n)
  local a = { 1234, 5678, 91 }
  local s = 0
  for k = 1, n do
    local r = mul_small(a, k % 97 + 1)
    s = s + #r + r[1]
  end
  return s
end
local s = work(6000)
for c = 1, 400 do
  local co = coroutine.wrap(function() return work(60) end)
  s = s + co()
end
print(s)
"##,
    },
    Prog {
        name: "c36_dom",
        small: true,
        src: r##"
-- C36 (`domtop`): the loop root is recorded on a rare path: the iteration recorded first takes the cold branch, which only every 16th iteration takes
-- afterwards (6 %), so about 94 % of the entries leave the root at the branch guard into one side trace (the shape of awfy/bounce at `wgate=45000`).
-- The rule re-records the root once.
local x = 0
for i = 1, 60000 do
  if i % 16 == 1 then
    x = (x + 3) % 10007
  else
    x = (x * 5 + i) % 10007
  end
end
print(x)
"##,
    },
    Prog {
        name: "c39_inner",
        small: true,
        src: r##"
-- C39 (`innerretry`): the loop root of the main chunk first records through a callee with a `while` loop of data-dependent trip count (`loopany` is off here:
-- the callee loop aborts the recording `InnerLoop`; two of them blacklisted the head for good). After 3000 iterations the callee is replaced by a loop-free
-- one, so a later retry round records a trace.
local function slow(i)
  local k, s = i % 5, 0
  while k > 0 do s = s + k; k = k - 1 end
  return s
end
local function fast(i) return i % 7 end
local g = slow
local total = 0
for i = 1, 60000 do
  if i == 3000 then g = fast end
  total = total + g(i)
end
print(total)
"##,
    },
    Prog {
        name: "bc_sbump",
        small: true,
        src: r##"
-- BUNDLE-C (`sbump`): a recursive walk over a tree of splits whose depth varies by data (the shape of regex_nfa's addState): the loop root inlines the recursion
-- to the unroll limit and ends in a stitch exit there; the side attempt at that exit starts AT the limit and is too short unless it gets one more unroll.
local states = {}
local function node(d)
  local s = { kind = "leaf" }
  states[#states + 1] = s
  local id = #states
  if d > 0 then
    s.kind = "split"
    s.o1 = node(d - 1)
    s.o2 = (d % 2 == 0) and node(d - 1) or node(0)
  end
  return id
end
local root = node(5)
local function add(list, onl, s, gen)
  if onl[s] == gen then return end
  onl[s] = gen
  local st = states[s]
  if st.kind == "split" then
    add(list, onl, st.o1, gen)
    add(list, onl, st.o2, gen)
  else
    list[#list + 1] = s
  end
end
local onl, total = {}, 0
for gen = 1, 4000 do
  local list = {}
  add(list, onl, root, gen)
  total = total + #list
end
print(total, #states)
"##,
    },
    Prog {
        name: "bc_fnpromo",
        small: true,
        src: r##"
-- BUNDLE-C (`fnpromo`): `work` is a long loop-free function entered from `drive` (a tail-recursive driver the loop root cannot inline past the unroll limit), so its
-- entry head gets hot from untraced code and may be promoted to a generic function root; a type change of the table `t` in the middle forces re-records.
local t = {}
for i = 1, 64 do t[i] = { k = i * 7 % 97, j = i % 13, w = (i % 7) * 0.5 } end
local function work(i)
  local a, b = t[i % 64 + 1], t[(i * 3) % 64 + 1]
  local x = a.k * 3 + a.j * 2 + a.w
  local y = b.k * 3 + b.j * 2 + b.w
  if x > y then x, y = y, x end
  a.w = (a.w + 0.25) % 4
  return x + y * 0.5 + (b.j - a.j)
end
local function drive(n, acc)
  if n == 0 then return acc end
  return drive(n - 1, acc + work(n))
end
local acc = 0
for r = 1, 8000 do
  if r == 5000 then for i = 1, 64 do t[i].k = t[i].k + 0.5 end end
  acc = acc + drive(60, 0)
end
print(acc)
"##,
    },
    Prog {
        name: "c38_side",
        small: true,
        src: r##"
-- C38 slice 1 (`sidenest`): `scan` (a `while` loop of data-dependent trip count) gets its own loop root first; the main loop's root then records the
-- `i % 5` branch that does not call it, and the side trace of the branch that does reaches scan's loop head inside the inlined frame: with `sidenest=1`
-- it ends with a linking exit into scan's trace instead of unrolling the loop; the callee returns into the caller loop body afterwards.
local function scan(a, lo, hi)
  local i, s = lo, 0
  while i <= hi do
    local v = a[i]
    if v % 3 == 0 then a[i] = v + 1 end
    s = s + v
    i = i + 1
  end
  return s
end
local a = {}
for i = 1, 40 do a[i] = i * 7 % 23 end
local r = 0
for k = 1, 400 do r = r + scan(a, 1, 40) end
local t = 0
for i = 1, 6000 do
  if i % 5 == 0 then t = t + scan(a, 1, 1 + i % 37) else t = t + i end
end
print(r, t)
"##,
    },
    Prog {
        name: "c38_side2",
        small: true,
        src: r##"
-- C38 slice 1: two inlined frames at the link (a wrapper calls the loop function), a float accumulator and a type change of the loop's table
-- halfway (the callee's loop trace is entered through the link with values it must re-check), a callee loop that exits by `break`.
local function find(a, x, hi)
  local i = 1
  while i <= hi do
    if a[i] == x then break end
    i = i + 1
  end
  return i
end
local function wrap(a, x, hi) return find(a, x, hi) * 2 end
local a = {}
for i = 1, 50 do a[i] = i end
local r = 0
for k = 1, 300 do r = r + find(a, (k % 40) + 1, 50) end
local t, f = 0, 0.5
for i = 1, 8000 do
  if i == 4000 then for j = 1, 50 do a[j] = j + 0.0 end end
  if i % 3 == 0 then t = t + wrap(a, (i % 45) + 1, 50) else f = f + 0.25 end
end
print(r, t, f)
"##,
    },
    Prog {
        name: "tlen_inline",
        small: true,
        src: r##"
-- the inline `#t` (trace TLEN / tier-1 LEN): stack push/pop, stale hints, holes, hash-part integers, boundaries, __len, no array part
local N = 300
local st, acc = {}, 0
for r = 1, N do
  for i = 1, 12 do st[#st + 1] = i end
  acc = acc + #st
  for i = 1, 12 do st[#st] = nil acc = acc + #st end
end
print(acc, #st)
local t, s = {}, 0
for i = 1, 200 do t[#t + 1] = i s = s + #t end
for i = 200, 1, -1 do s = s + #t t[i] = nil end
print(s, #t)
-- a full array with integers in the hash part: the border is beyond the array
local hh = { 1, 2, 3, [4] = 4, [5] = 5 }
local hs = 0
for r = 1, N do hs = hs + #hh end
print(hs)
-- a hint on a hole
local ho = {}
for i = 1, 16 do ho[i] = i end
local hr = {}
for r = 1, 40 do
  for k = 1, 16 do
    ho[k] = nil
    hr[#hr + 1] = #ho
    ho[k] = k
  end
end
local hsum = 0
for i = 1, #hr do hsum = hsum + hr[i] end
print(hsum, #hr)
-- no array part / empty / hash only
local e, ek, z = {}, { a = 1, b = 2 }, 0
for r = 1, N do z = z + #e + #ek end
ek[1] = 1
print(z, #ek)
-- sizes at array boundaries (hint == asize with a dummy hash part, and with a populated one)
local tot = 0
for r = 1, 8 do
  for _, n in ipairs { 1, 2, 3, 4, 5, 8, 9, 16, 17, 32, 33 } do
    local a = {}
    for i = 1, n do a[i] = true end
    local x = 0
    for q = 1, 30 do x = x + #a end
    a.k = 1
    for q = 1, 30 do x = x + #a end
    tot = tot + x
  end
end
print(tot)
-- __len, a metatable without it, strings and numbers in one loop
local mt = setmetatable({ 1, 2, 3 }, { __len = function() return 42 end })
local plain = { 1, 2, 3 }
local withmt = setmetatable({ 1, 2, 3, 4 }, { __index = function() return 0 end })
local objs = { plain, mt, withmt, "hello", plain, {}, { 1 }, mt }
local sum = 0
for r = 1, N do
  for k = 1, #objs do sum = sum + #objs[k] end
end
print(sum)
local ok, err = pcall(function() local v = 5; for r = 1, 100 do local _ = #v end end)
print(ok, (err:gsub("^.-:%d+: ", "")))
-- a loop where the same code first sees a plain table and later one with __len
local u = { 1, 2, 3 }
local us = 0
for r = 1, 3000 do
  if r == 2000 then setmetatable(u, { __len = function() return 7 end }) end
  us = us + #u
end
print(us)
-- random sets / clears / pushes / pops of a 32-slot array with holes (no key beyond slot 32: the hash part stays empty, so every border is
-- defined by the hint alone); `#` after every step: the hint must move exactly as the helper moves it
local rseed = 12345
local function rnd(n) rseed = (rseed * 1103515245 + 12345) % 2147483648 return rseed % n + 1 end
local function fill32() local a = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32 } return a end
local rt, rsum = fill32(), 0
for step = 1, 6000 do
  local op = rnd(9)
  if op <= 2 then if #rt < 32 then rt[#rt + 1] = step end
  elseif op <= 4 then local n = #rt if n > 0 then rt[n] = nil end
  elseif op == 5 then rt[rnd(32)] = step
  elseif op == 6 then rt[rnd(32)] = nil
  elseif op == 7 then rt[rnd(6)] = nil
  elseif op == 8 then local n = #rt if n < 31 then rt[n + 1] = step rt[n + 2] = step end
  else local c0 = rnd(26) for k = 0, 5 do rt[c0 + k] = nil end end
  rsum = rsum + #rt * (step % 7 + 1)
  if step % 1500 == 0 then rt = fill32() end
end
print(rsum, #rt)
-- fresh constructor tables (hint 0): all set, a nil at each position, sizes around the five-slot limit of the helper's upward search
local zs = 0
for i = 1, 400 do
  local a = { i }
  local b = { i, i }
  local c = { i, i, nil }
  local d = { i, nil, i }
  local e = { nil, i }
  local f = { i, i, i, i, i }
  local g = { i, i, i, i, i, i }
  local h = { i, i, i, i, i, i, nil, i }
  local k = { i, i, i, i, nil, i }
  local m = { i, i, i, i, i, nil }
  local n13 = { i, i, i, i, i, i, i, i, i, i, i, i, i, nil, i, i }
  local n7 = { i, i, i, i, i, i, nil, i, i }
  zs = zs + #n13 * 31 + #n7 * 37
  zs = zs + #a + #b * 3 + #c * 5 + #d * 7 + #e * 11 + #f * 13 + #g * 17 + #h * 19 + #k * 23 + #m * 29
  b[3] = i
  zs = zs + #b + #f
end
print(zs)
-- GETI / SETI with the constant key 0 (tier 1: the helper, not an exit): plain, __index / __newindex, nil, non-table
local k0, ks = {}, 0
for i = 1, 600 do k0[0] = (i % 3 ~= 0) and i or nil if k0[0] then ks = ks + k0[0] end end
local k0log = {}
local k0m = setmetatable({}, { __newindex = function(t, k, v) k0log[#k0log + 1] = v rawset(t, k, v) end, __index = function(t, k) return -1 end })
for i = 1, 300 do ks = ks + k0m[0] if i == 150 then k0m[0] = 5 end end
print(ks, #k0log, k0m[0])
print(pcall(function() local v = true; for i = 1, 50 do v[0] = i end end))
print(pcall(function() local v = 7; local x; for i = 1, 50 do x = v[0] end end))
"##,
    },
    Prog {
        name: "p3_chain",
        small: true,
        src: r##"
-- P3 (`chainre`): a toy interpreter dispatching `word.fn()` over 12 distinct primitives. The first 60 words visit the primitives in order 1..12 (the chain
-- of side traces recorded then has 9..12 last), phase 1 is dominated by 9..12 (1..8 are rare), phase 2 by 1..4: the distribution changes mid-run.
local acc, steps = 0, 0
local words = {}
for i = 1, 12 do
  local f = load(("return function() return %d end"):format(i * 3 % 7 + i))()
  words[i] = { kind = "prim", fn = f, id = i }
end
local vword = { kind = "var", addr = 5 }
local function execute(word)
  steps = steps + 1
  if word.kind == "prim" then
    acc = (acc + word.fn()) % 1000003
  elseif word.kind == "var" then
    acc = acc + word.addr
  end
end
local seed = 12345
local function rnd()
  seed = (seed * 16807) % 2147483647
  return seed // 16
end
local function run(phase, n)
  local prog = {}
  for i = 1, 1024 do
    local r = rnd()
    local w
    if phase == 0 then
      w = words[(i - 1) % 12 + 1]
    elseif r % 16 == 0 then
      w = vword
    elseif r % 1000 < 3 then
      w = words[(r // 1000) % 8 + (phase == 1 and 1 or 5)]
    elseif phase == 1 then
      w = words[9 + (r // 7) % 4]
    else
      w = words[1 + (r // 7) % 4]
    end
    prog[i] = w
  end
  for rep = 1, n do
    for i = 1, 1024 do execute(prog[i]) end
  end
end
run(0, 1)
run(1, 40)
run(2, 40)
print(acc, steps)
"##,
    },
    Prog {
        name: "p3_uniform",
        small: true,
        src: r##"
-- P3 (`chainre`, the thrash bound): the dispatch of `p3_chain` with 4 callees (and a third kind of word) used uniformly (pseudo-random order, so the order of the chain is as good as any other):
-- no re-order may fire however often the chain is checked, even when the pass-through test is lowered to 60 % (the first link passes 75 % of its entries through).
local acc, steps = 0, 0
local words = {}
for i = 1, 12 do
  local f = load(("return function() return %d end"):format(i * 3 % 7 + i))()
  words[i] = { kind = "prim", fn = f, id = i }
end
local vword = { kind = "var", addr = 5 }
local function execute(word)
  steps = steps + 1
  if word.kind == "prim" then
    acc = (acc + word.fn()) % 1000003
  elseif word.kind == "var" then
    acc = acc + word.addr
  end
end
local seed = 777
local prog = {}
for i = 1, 1024 do
  seed = (seed * 16807) % 2147483647
  local r = seed // 16
  prog[i] = (r % 4 == 0) and vword or words[(r // 16) % 4 + 1]
end
for rep = 1, 300 do
  for i = 1, 1024 do execute(prog[i]) end
end
print(acc, steps)
"##,
    },
    Prog {
        name: "p2_fconst",
        small: true,
        src: r##"
-- P2-3c (`fpool`): double constants from the per-state pool; `x * 2.0` as `x + x` must be bit-exact for every double (NaN payload and sign, zeros,
-- infinities, denormals, overflow), `x * 4.0` must stay a multiplication, constants in compares and as the first operand.
local vals = {0.0, -0.0, 1/0, -1/0, 0/0, -(0/0), 2^53, -2^53, 1e308, -1e308, 5e-324, -5e-324, 0.1, 2.5e-308, 3.0, 0.5}
local function f(x, n)
  local a, b, c, d, e, g = 0.0, 0.0, 0.0, 0.0, 0.0, 0.0
  for i = 1, n do
    a = x * 2.0
    b = 2.0 * x
    c = x * 4.0
    d = x * 0.5 + 1.25
    e = (x < 2.5) and 1.0 or -1.0
    g = 7.5 - x / 3.0
  end
  return a, b, c, d, e, g
end
for i = 1, #vals do
  local a, b, c, d, e, g = f(vals[i], 300)
  print(i, string.format("%a %a %a %a %a %a", a, b, c, d, e, g))
end
local s = 0.0
for i = 1, 3000 do s = s + i * 2.0 + 0.25 * s * 1e-6 - 1.5 end
print(string.format("%a", s))
local m = 0
for i = 1, 2000 do local z = i * 1.5; if z * 2.0 > 2000.5 and z < 2.5e3 then m = m + 1 end end
print(m)
"##,
    },
    Prog {
        name: "bg_wide",
        small: true,
        src: r##"
-- BUNDLE-G (`chaindepth`): one call site, 24 distinct closures used uniformly in pseudo-random order (a chain of callee-guard side traces as long as the family
-- allows), then the table is replaced under the loop and the loop runs on.
local K = 24
local fs = {}
local function fill(off)
  for i = 1, K do fs[i] = load(("return function(x) return (x + %d) %% 1000003 end"):format(i + off))() end
end
fill(0)
local seed = 4242
local ops = {}
for i = 1, 1024 do seed = (seed * 16807) % 2147483647; ops[i] = (seed // 64) % K + 1 end
local acc = 0
for r = 1, 200 do
  for i = 1, 1024 do acc = fs[ops[i]](acc) end
end
fill(100)
for r = 1, 100 do
  for i = 1, 1024 do acc = fs[ops[i]](acc) end
end
print(acc)
"##,
    },
    Prog {
        name: "bg_narrow",
        small: true,
        src: r##"
-- BUNDLE-G (`chaindepth`): the same with 4 closures: the first links serve every call (the coverage test never cuts this chain).
local K = 4
local fs = {}
for i = 1, K do fs[i] = load(("return function(x) return (x + %d) %% 1000003 end"):format(i))() end
local seed = 99
local ops = {}
for i = 1, 1024 do seed = (seed * 16807) % 2147483647; ops[i] = (seed // 64) % K + 1 end
local acc = 0
for r = 1, 300 do
  for i = 1, 1024 do acc = fs[ops[i]](acc) end
end
print(acc)
"##,
    },
    Prog {
        name: "p3_fdisp",
        small: true,
        src: r##"
-- P3 (`ncallee`): a call through a table of 4 distinct closures in a pseudo-random order: the callee guard in the entry segment fails 3 entries in 4
-- (entry-failure flushes and finally a blacklisted loop); with the guard kept out of the entry segment the loop keeps its trace and the mismatches are
-- ordinary exits with side traces. Then the table's contents are replaced (all four callees change) and the loop runs on.
local K = 4
local fs = {}
for i = 1, K do fs[i] = load(("return function(x) return (x + %d) %% 1000003 end"):format(i))() end
local seed = 12345
local ops = {}
for i = 1, 512 do
  seed = (seed * 16807) % 2147483647
  ops[i] = (seed // 65536) % K + 1
end
local acc = 0
local function run(n)
  for r = 1, n do
    for i = 1, 512 do acc = fs[ops[i]](acc) end
  end
end
run(60)
for i = 1, K do fs[i] = load(("return function(x) return (x * 3 + %d) %% 1000003 end"):format(i))() end
run(60)
print(acc)
"##,
    },
    Prog {
        name: "gm_loops",
        small: true,
        // BUNDLE-GM: `gmatch` iterators in generic-for position (the vm corpus script: captures, empty matches, anchors, init, nesting, errors, coroutines, GC pressure)
        src: include_str!("../../../../tests/vm/strings_22_gmatch_loops.lua"),
    },
    Prog {
        name: "p2_xregs_rec",
        small: true,
        // P2 bundle 1 (`xregs`): native recursion (function-entry traces whose frame push reads `r14`: they must never allocate it) with many live values
        // across the calls and a base case that runs in a side trace / tier 1.
        src: r##"
local function rr(n, a, b, c, d, e, g, h, i, j, k, l)
  if n < 2 then return a + b + c + i + j + k + l end
  local x = rr(n - 1, b, c, d, e, g, h, i, j, k, l, a + 1)
  local y = rr(n - 2, c, d, e, g, h, i, j, k, l, a, b + 1)
  return x + y + a * b + c * d + e * g + h + i * j + k * l + n % 5
end
local function run()
  local r = 0
  for q = 1, 40 do
    r = r + rr(7 + q % 4, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11)
    r = r + #string.format("%d", r)
  end
  return r
end
print(run())
"##,
    },
    Prog {
        name: "p2_xregs",
        small: true,
        // P2 bundle 1 (`xregs`): `rbp` and `r14` as allocatable trace registers. Nine loop-carried integers plus a `%` (takes `rdx` away) and a variable shift
        // (takes `rcx` away) leave no register without them; a helper call (`math.type`, `string.byte`) in the loop keeps values live across `CallH`;
        // guards fail on ordinary exits (the landing code reads string constants: `r14` must be the constants of the frame again), a float variant,
        // a function-entry trace (native recursion: its `Lea K` rules `r14` out) and a callee that runs in tier 1 after the loop exits.
        src: r##"
local function f(n, t)
  local a, b, c, d, e, g, h, m, k = 1, 2, 3, 4, 5, 6, 7, 8, 9
  for i = 1, n do
    a = a + i
    b = b + a % 7
    c = c + b
    d = d + (c & 255)
    e = e + (d >> (i & 3))
    g = g + (e ~ a)
    h = h + i * 2 + #t[1 + i % 3]
    m = m + (math.type(i) == "integer" and 1 or 0)
    k = k + t[1 + i % 3]:byte(1)
    if i % 61 == 0 then a = a - 1 elseif i % 17 == 0 then b = b + 3 end
  end
  return a, b, c, d, e, g, h, m, k
end
local function g2(n, x)
  local s, u, v, w = 0.0, 1.5, 2.5, 3.5
  local cnt = 0
  for i = 1, n do
    s = s + x * u - v / (w + i)
    u = u * 0.999 + s * 1e-6
    v = v + (i % 5) * 0.25
    w = w + 1.0
    if s > 1e30 then cnt = cnt + 1 end
  end
  return s, u, v, w, cnt
end
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
-- inlined callee frames with many live values in the caller loop: the exits rebuild frames (`mat_stub`) and land in tier 1, which reads constants
local function step(x, y, k)
  if x % 97 == 0 then return y + 123456789012 + #k end
  return x + y * 3
end
local function inl(n)
  local a, b, c, d, e, g, h, m = 1, 2, 3, 4, 5, 6, 7, 8
  for i = 1, n do
    a = step(a, i, "kk")
    b = b + a % 7
    c = c + step(b, 1, "abc")
    d = d + (c & 255)
    e = e + (d >> (i & 3))
    g = g + (e ~ a)
    h = h + i * 2
    m = m + step(i, g & 7, "q")
  end
  return a, b, c, d, e, g, h, m
end
local t = {"x", "yy", "zzz"}
local r = 0
for q = 1, 30 do
  local ia, ib, ic, id, ie, ig, ih, im = inl(90 + q)
  r = r + ia + ib + ic + id + ie + ig + ih + im

  local a, b, c, d, e, g, h, m, k = f(100 + q, t)
  r = r + a + b + c + d + e + g + h + m + k
  local s = string.format("%d:%d", a, b)
  r = r + #s + fib(10 + q % 3)
  local fs, fu, fv, fw = g2(60 + q, q * 0.5)
  r = r + #string.format("%.6g %.6g %.6g %.6g", fs, fu, fv, fw)
end
print(r, f(1000, t))
print(string.format("%.9g %.9g %.9g %.9g %d", g2(500, 1.25)))
"##,
    },
    Prog {
        name: "p2_uvsep",
        small: true,
        src: r##"
-- P2 bundle 2 (`uvsep`): closures of one prototype with different upvalue cells (recorded as different `UpVal`s); later calls pass closures that share a cell
-- (the getter of the incrementer's own cell, a `debug.upvaluejoin`ed pair): the load behind two stores must see the stores.
local function mk(start, step)
  local n = start
  return function() n = n + step; return n end, function() return n end
end
local inc1, get1 = mk(1, 1)
local inc2, get2 = mk(100, 2)
local inc3, get3 = mk(1000, 3)
local function run(f, g, h, gh, rounds)
  local s = 0
  for i = 1, rounds do
    local a = gh()
    f()
    g()
    local b = gh()
    s = s + (b - a) * 3 + h() % 7
  end
  return s
end
for k = 1, 4 do
  print("distinct", run(inc1, inc2, inc3, get3, 150), get1(), get2(), get3())
end
for k = 1, 3 do
  print("alias f", run(inc1, inc2, inc3, get1, 120), get1(), get2(), get3())
  print("alias g", run(inc1, inc2, inc3, get2, 120), get1(), get2(), get3())
  print("distinct", run(inc1, inc2, inc3, get3, 120), get1(), get2(), get3())
end
debug.upvaluejoin(get3, 1, inc1, 1)
for k = 1, 3 do
  print("joined", run(inc1, inc2, inc3, get3, 130), get1(), get2(), get3())
  print("joined2", run(inc3, inc2, inc1, get1, 130), get1(), get2(), get3())
end
-- the same cell twice in the store position
for k = 1, 3 do
  print("same f g", run(inc1, inc1, inc3, get1, 100), get1(), get2(), get3())
end
"##,
    },
    Prog {
        name: "p2_uvroll",
        small: true,
        src: r##"
-- P2 bundle 2 (`uvroll`): traces with table constructors and key insertions (header-mutating ops) that also read and write upvalues: the upvalue facts
-- survive the constructors, the table facts do not (a table that grows between two reads of its array).
local n, acc = 0, 0
local big = {}
for i = 1, 16 do big[i] = i end
local function mk(i)
  n = n + 1
  local t = {n, i, x = n}
  acc = acc + t[1] + t.x
  n = n + t[2] % 3
  return t
end
local function grow(i)
  local t = {}
  for k = 1, 3 do t[k] = k * i end
  local before = t[2]
  t.name = i
  t.other = i + 1
  t[#t + 1] = n
  n = n + #t
  big[i % 16 + 1] = big[i % 16 + 1] + 1
  return before + t[2] + n + big[(i + 3) % 16 + 1]
end
local function loop(rounds)
  local last, s = nil, 0
  for i = 1, rounds do
    last = mk(i)
    acc = acc + n
    s = s + grow(i)
    acc = acc + n % 5
  end
  return last, s
end
for k = 1, 5 do
  local last, s = loop(120)
  print(k, n, acc, last[1], last[2], last.x, s)
end
local box = {v = 0}
local function bump() box.v = box.v + 1; n = n + box.v; local c = {n}; c[2] = acc; return c[1] + c[2] end
local tot = 0
for i = 1, 400 do tot = tot + bump() end
print(tot, n, acc, box.v)
"##,
    },
    Prog {
        name: "r1_rearm",
        small: false,
        // R1 (`rearm`): a loop (`run`, called 6000 times so that its trace is entered often) whose callee leaves compiled code every 10th iteration at a lookup that runs an
        // `__index` function with a 40-iteration loop (not inlinable): the interpreter finishes the call until the next sampling tick (1000 back-edges); the rearmed tick puts
        // it back into the trace.
        src: r##"
local calls = 0
local base = setmetatable({}, {__index = function(t, k)
  local n = 0
  for j = 1, 40 do n = n + j; calls = calls + 1 end
  return n + select('#', k, t)
end})
local obj = setmetatable({}, {__index = base})
local plain = {val = 3}
local function get(o, k) return o[k] end
local function run(n)
  local s = 0
  for i = 1, n do
    local o = plain
    if i % 10 == 0 then o = obj end
    s = s + get(o, "val") + (i & 3)
  end
  return s
end
local s = 0
for k = 1, 6000 do s = s + run(40) end
print(s, calls)
"##,
    },
    Prog {
        name: "r1_rearm_warm",
        small: false,
        // R1: the same exits, but in a second loop (a Warm head: 200 iterations) behind a hot traced loop of the same prototype: the tick is armed (the prototype
        // holds a trace) and finds no traced head at the back-edges it fires at.
        src: r##"
local calls = 0
local base = setmetatable({}, {__index = function(t, k)
  local n = 0
  for j = 1, 3 do n = n + j; calls = calls + 1 end
  return n + select('#', k, t)
end})
local obj = setmetatable({}, {__index = base})
local plain = {val = 3}
local function get(o, k) return o[k] end
local s = 0
for i = 1, 30000 do
  s = s + get(plain, "val") + (i & 3)
end
for i = 1, 200 do
  local o = plain
  if i % 4 == 0 then o = obj end
  s = s + get(o, "val") + (i & 3)
end
print(s, calls)
"##,
    },
    Prog {
        name: "r1_rearm_hook",
        small: false,
        // R1: a count hook (never fires) is installed first: a thread with hooks never arms the tick.
        src: r##"
debug.sethook(function() end, "", 100000000)
local calls = 0
local base = setmetatable({}, {__index = function(t, k)
  local n = 0
  for j = 1, 3 do n = n + j; calls = calls + 1 end
  return n + select('#', k, t)
end})
local obj = setmetatable({}, {__index = base})
local plain = {val = 3}
local function get(o, k) return o[k] end
local s = 0
for i = 1, 40000 do
  local o = plain
  if i % 50 == 0 then o = obj end
  s = s + get(o, "val") + (i & 3)
end
debug.sethook()
print(s, calls)
"##,
    },
    Prog {
        name: "r1_rearm_co",
        small: false,
        // R1: the same inside a coroutine (the frames and the tick belong to another thread), yielding now and then.
        src: r##"
local calls = 0
local base = setmetatable({}, {__index = function(t, k)
  local n = 0
  for j = 1, 40 do n = n + j; calls = calls + 1 end
  return n + select('#', k, t)
end})
local obj = setmetatable({}, {__index = base})
local plain = {val = 3}
local function get(o, k) return o[k] end
local function run(n)
  local s = 0
  for i = 1, n do
    local o = plain
    if i % 10 == 0 then o = obj end
    s = s + get(o, "val") + (i & 3)
  end
  return s
end
local co = coroutine.wrap(function()
  local s = 0
  for k = 1, 6000 do
    s = s + run(40)
    if k % 700 == 0 then coroutine.yield(s) end
  end
  return s
end)
local r
for k = 1, 9 do r = co() end
print(r, calls)
"##,
    },
    Prog {
        name: "ba_num",
        small: true,
        src: r####"
-- BUNDLE-A `libnum`: tonumber(string) recorded (STRTO in tonumber mode): int / float / hex / whitespace / overflow-to-float numerals, non-numerals, a type change in the middle.
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
-- piece 1: tonumber(string)
local strs = { "10", " 12 ", "0x1F", "1e2", "3.5", "9223372036854775807", "9223372036854775808", "-0x10", "  7", "1e", "abc", "", "0x", "5 5", "1.", ".5", "inf", "nan", "0x.8p1", "1_0" }
local function numeric(n)
  local acc = 0
  for i = 1, n do
    local v = tonumber("42") + tonumber("  0x10  ") + tonumber("1e2") + tonumber("2.5")
    acc = acc + v
  end
  return acc
end
P(numeric(375))
local function mixed(n, s)
  local si, sf, cnt = 0, 0.0, 0
  for i = 1, n do
    local v = tonumber(s[i % #s + 1])
    if v then cnt = cnt + 1; if math.type(v) == "integer" then si = si + v else sf = sf + v end end
  end
  return si, sf, cnt
end
P(mixed(625, { "10", "20", "30" }))
P(mixed(625, { "10", "2.5", "30", "1e1" }))
P(mixed(625, { "10", "x", "30" }))
P(mixed(625, { "10", "9223372036854775808", "0x7fffffffffffffff" }))
P(mixed(250, strs))
local function allint(n, s) local a = 0 for i = 1, n do a = a + tonumber(s) end return a end
P(allint(500, "7"), allint(500, "  0x1f "))
-- type change in a long loop (exit on the type guard)
local function chg(n) local a = 0 for i = 1, n do local s = i < 375 and "5" or "5.5"; a = a + tonumber(s) end return a end
P(chg(750))
local function nonnum(n) local a = 0 for i = 1, n do local s = i < 375 and "5" or "zz"; a = a + (tonumber(s) or 100) end return a end
P(nonnum(750))

print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "ba_next",
        small: true,
        src: r####"
-- BUNDLE-A `libnext`: the plain call next(t [, k]): empty / non-empty tables, a full traversal, keys deleted during the traversal, a replaced `next` global, invalid keys.
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
-- piece 2: next
local function nx(t, n)
  local acc = 0
  for i = 1, n do
    local k, v = next(t)
    if k then acc = acc + v end
  end
  return acc
end
P(nx({}, 625), nx({ 5, 6 }, 625), nx({ a = 3 }, 625))
local function walk(t, n)
  local acc = 0
  for i = 1, n do
    local k, v = next(t)
    while k do acc = acc + v; k, v = next(t, k) end
  end
  return acc
end
P(walk({ 1, 2, 3, 4 }, 375), walk({ a = 1, b = 2, c = 3 }, 375), walk({ 1, 2, x = 5, y = 6, 7 }, 375))
local function walkdel(n)
  local acc = 0
  for i = 1, n do
    local t = { a = 1, b = 2, c = 3, d = 4, 1, 2 }
    local k, v = next(t)
    while k do acc = acc + v; t[k] = nil; k, v = next(t, k) end
  end
  return acc
end
P(walkdel(250))
local function emptycheck(t, n) local c = 0 for i = 1, n do if next(t) == nil then c = c + 1 end end return c end
P(emptycheck({}, 625), emptycheck({ 1 }, 625))
local function keyonly(t, n) local c = 0 for i = 1, n do c = c + (next(t, 2) or 0) end return c end
P(keyonly({ 1, 2, 3 }, 500))
local function stmt(t, n) for i = 1, n do next(t) end return 1 end
P(stmt({ 1 }, 500))
local function multret(t, n) local c = 0 for i = 1, n do local a, b, d = next(t) c = c + (b or 0) end return c end
P(multret({ 9 }, 500))
local function swap(n)
  local t1, t2 = { 1 }, {}
  local c = 0
  for i = 1, n do local k = next(i % 2 == 0 and t1 or t2); if k then c = c + 1 end end
  return c
end
P(swap(750))
-- user-replaced next
local function usern(n, t)
  local c = 0
  for i = 1, n do local k = next(t); c = c + (k or 0) end
  return c
end
P(usern(500, { 7 }))
local realnext = next
next = function(t, k) return 1000, 1 end
P(usern(500, { 7 }))
next = realnext
P(usern(500, { 7, 8, 9 }))
next = rawlen -- another light C function: the callee type is the same, only its identity differs
P(usern(500, { 7, 8, 9 }))
P(usern(500, { 7, 8, 9 }))
-- the callee comes from data: another light C function (same type) after 437 iterations; only its identity tells it from `next`
local fns = { next, rawlen }
local function viadata(n) local c = 0 for i = 1, n do local k = fns[1 + (i // 437) % 2]({ 7, 8, 9 }); c = c + (k or 0) end return c end
P(viadata(1000))
P(pcall(function() local c = 0 for i = 1, 625 do c = c + (next({ 1 }, i < 500 and nil or "nokey") or 0) end return c end))
P(pcall(function() for i = 1, 625 do next(i < 500 and {} or 5) end end))

print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "ba_find",
        small: true,
        src: r####"
-- BUNDLE-A `libfind`: plain string.find: plain true / false / nil / other truthy, specials in the pattern at run time, init negative / over-length / nil, empty pattern, found / not found alternation, long strings, multi results.
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
-- piece 3: find
local function f(s, p, init, plain, n)
  local a, b = 0, 0
  for i = 1, n do
    local x, y = string.find(s, p, init, plain)
    if x then a = a + x; b = b + y else a = a - 1 end
  end
  return a, b
end
local text = "hello world, hello lua"
P(f(text, "world", 1, true, 500))
P(f(text, "world", nil, true, 500))
P(f(text, "xyz", 1, true, 500))
P(f(text, "o", -5, true, 500))
P(f(text, "o", -100, true, 500))
P(f(text, "o", 22, true, 500))
P(f(text, "o", 23, true, 500))
P(f(text, "o", 24, true, 500))
P(f(text, "", 1, true, 500))
P(f(text, "", 23, true, 500))
P(f(text, "", 24, true, 500))
P(f(text, "lua", 1, false, 500))
P(f(text, "lua", 1, nil, 500))
P(f(text, "l.a", 1, false, 500))
P(f(text, "l.a", 1, true, 500))
P(f(text, "%s", 1, true, 500))
P(f(text, "%s", 1, false, 500))
P(f(text, "o", 1, 1, 500))
P(f(text, "o", 1, "x", 500))
local function two(s, p, n) local a = 0 for i = 1, n do local x = s:find(p, 1, true); a = a + (x or 0) end return a end
P(two(text, "w", 500), two(text, "q", 500))
-- alternation found / not found
local function alt(n) local a = 0 for i = 1, n do local x, y = text:find(i % 3 == 0 and "zzz" or "lo", 1, true) a = a + (x or -1) + (y or -2) end return a end
P(alt(750))
-- specials appear at run time in a non-plain loop
local pats = { "lo", "hello", "w", "l.a" }
local function sp(n) local a = 0 for i = 1, n do local x, y = text:find(i < 437 and "lo" or pats[i % 4 + 1]) a = a + (x or -1) + (y or -2) end return a end
P(sp(1000))
local function sp2(n) local a = 0 for i = 1, n do local x, y = text:find(i < 437 and "lo" or "l(o)", 1) a = a + (x or -1) + (y or -2) end return a end
P(sp2(1000))
-- recorded "not found" for a pattern without specials, then a pattern with specials that does match
local pp = { "zzz", "l+" }
local function sp3(n) local a = 0 for i = 1, n do local x, y = text:find(pp[1 + (i // 437) % 2], 1) a = a + (x or -1) + (y or -2) end return a end
P(sp3(1000))
-- long strings
local long = string.rep("abc", 100) .. "needle" .. string.rep("xyz", 100)
local function lf(n, p) local a = 0 for i = 1, n do local x, y = long:find(p, 1, true); a = a + (x or -1) + (y or -2) end return a end
P(lf(375, "needle"), lf(375, string.rep("c", 3)), lf(375, string.rep("abc", 100) .. "needle"))
local function tp(n) local a = 0 for i = 1, n do local x, y = text:find(i < 375 and "lo" or string.rep("lo", 3), 1, true) a = a + (x or -1) + (y or -2) end return a end
P(tp(750))
local function nstr(n) local a = 0 for i = 1, n do local x = ("a1b2"):find(1, 1, true); a = a + (x or -1) end return a end
P(pcall(nstr, 500))
local function fch(n) local a = 0 for i = 1, n do local x, y = text:find("l", i % 30, true); a = a + (x or -1) + (y or -2) end return a end
P(fch(750))
P(select("#", text:find("o", 1, true)), select("#", text:find("zz", 1, true)))
local function ret3(n) local a = 0 for i = 1, n do local x, y, z = text:find("o", 1, true); a = a + y + (z or 0) end return a end
P(ret3(500))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "ba_num_floats",
        small: true,
        src: r####"
-- BUNDLE-A `libnum`: float / hex / exponent numerals in a hot loop (a trace that runs without leaving)
local function f(n)
  local acc = 0
  for i = 1, n do
    acc = acc + tonumber("2.5") + tonumber("1e2") + tonumber(" 0x10 ") + tonumber("7")
  end
  return acc
end
print(f(3000))
"####,
    },
    Prog {
        name: "tc_fmt",
        small: true,
        src: r####"
-- BUNDLE-C `tfmt`: string.format of constant formats of %d %x %X %s %c items (widths, - and 0 flags), recorded as a LIB helper.
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
local f = string.format
-- one to three items; integer, negative, extreme integers; widths and flags
local function a1(n, fmt) local acc, last = 0, "" for i = 1, n do local s = f(fmt, i * 7919 - 3000) acc = acc + #s last = s end return acc, last end
for _, fmt in ipairs { "%d", "%x", "%X", "%5d", "%-5d|", "%05d", "%08x", "%08X", "%c", "x%dy", "%%%d%%", "%2d", "%-2d|", "%99d" } do
  P(fmt, pcall(a1, 400, fmt))
end
local function ext(n) local acc = {} for i = 1, n do acc[#acc + 1] = f("%d|%x|%X", math.maxinteger - i, math.mininteger + i, -i) end return acc[1], acc[#acc] end
P(ext(300))
local function c1(n) local t = {} for i = 1, n do t[#t + 1] = f("%c", 32 + i % 90) end return table.concat(t) end
P(c1(300))
local function c2(n) local t = {} for i = 1, n do t[#t + 1] = f("%3c|%-3c|", 65 + i % 26, 97 + i % 26) end return #table.concat(t) end
P(c2(300))
-- %s: short strings, long strings, integers, widths; the result kind (short / long) changes in the middle
local words = { "a", "bb", "ccc", "", "dddddddddddddddddddddddddddddddddddddd" }
local function s1(n, fmt) local acc = 0 for i = 1, n do acc = acc + #f(fmt, words[i % 5 + 1]) end return acc end
for _, fmt in ipairs { "%s", "[%s]", "%5s", "%-5s|", "%10s", "%s%s" } do P(fmt, pcall(s1, 500, fmt)) end
local function s2(n) local acc = 0 for i = 1, n do acc = acc + #f("%s:%s:%s", words[i % 5 + 1], i, words[(i + 2) % 5 + 1]) end return acc end
P(s2(500))
local long = string.rep("q", 60)
local function s3(n) local acc = 0 for i = 1, n do acc = acc + #f("%s-%s", i < 300 and "ab" or long, "z") end return acc end
P(s3(600))
local function s4(n) local acc = 0 for i = 1, n do acc = acc + #f("%s", i < 300 and "ab" or long .. i) end return acc end
P(s4(600))
local function s5(n) local acc = 0 for i = 1, n do acc = acc + #f("%5s|%-7s|%3d", long, "r", i) end return acc end
P(s5(300))
-- mixed formats, argument counts and the format register changing between two constants
local function m1(n) local t = {} for i = 1, n do local up = i % 3 == 0 t[#t + 1] = f(up and "%08X" or "%08x", i * 40503) end return t[1], t[3], t[#t] end
P(m1(600))
local function m2(n) local acc = 0 for i = 1, n do acc = acc + #f(i < 200 and "%d" or "%d:%d", i, i) end return acc end
P(m2(500))
local function m3(n) local acc = 0 for i = 1, n do acc = acc + #f(i < 200 and "%d" or "%x", i * 255) end return acc end
P(m3(500))
local function m4(n) local acc = 0 for i = 1, n do local s = f("%s=%d", "k", i) acc = acc + #s + (s:byte(#s)) end return acc end
P(m4(500))
-- arguments that leave the recorded shape in the middle: floats (exact / inexact), strings, nil, missing, extra
local function d1(n, v) local acc = 0 for i = 1, n do acc = acc + #f("%d", i < 300 and i or v) end return acc end
P(pcall(d1, 600, 5))
P(pcall(d1, 600, 5.0))
P(pcall(d1, 600, 5.5))
P(pcall(d1, 600, "12"))
P(pcall(d1, 600, nil))
P(pcall(d1, 600, {}))
P(pcall(d1, 600, math.huge))
local function d2(n, v) local acc = 0 for i = 1, n do acc = acc + #f("%s|%d", i < 300 and "x" or v, i) end return acc end
P(pcall(d2, 600, "y"))
P(pcall(d2, 600, 7))
P(pcall(d2, 600, 7.5))
P(pcall(d2, 600, nil))
P(pcall(d2, 600, true))
P(pcall(d2, 600, {}))
P(pcall(d2, 600, "a\0b"))
local function d3(n) local acc = 0 for i = 1, n do acc = acc + #f("%5s|", i < 300 and "x" or "a\0b") end return acc end
P(pcall(d3, 600))
local function d4(n) local acc = 0 for i = 1, n do acc = acc + #f("%d %d", i, i < 300 and i or nil) end return acc end
P(pcall(d4, 600))
local function d5(n) local acc = 0 for i = 1, n do acc = acc + #f("%d", i, i < 300 and i or nil, "extra") end return acc end
P(pcall(d5, 600))
local function d6(n) local acc = 0 for i = 1, n do acc = acc + #f(i < 300 and "%d" or "%q", i) end return acc end
P(pcall(d6, 600))
local function d7(n) local acc = 0 for i = 1, n do acc = acc + #f(i < 300 and "%d" or "%123d", i) end return acc end
P(pcall(d7, 600))
local function d8(n) local acc = 0 for i = 1, n do acc = acc + #f("%.3d|%+d|% d|%#x", i, i, i, i) end return acc end
P(pcall(d8, 300))
P(pcall(f, "%d"))
P(pcall(f, "%s"))
P(pcall(f, "%5.2s", "abc"))
-- __tostring on the string metatable / a number metatable appearing in the middle of a %s loop
local function t1(n) local acc = 0 for i = 1, n do acc = acc + #f("%s", i < 300 and "plain" or "x") if i == 300 then getmetatable("").__tostring = function(s) return "T:" .. s end end end return acc end
P(t1(600))
getmetatable("").__tostring = nil
local function t2(n) local acc = 0 for i = 1, n do acc = acc + #f("%s|%s", i, "k") if i == 300 then debug.setmetatable(0, { __tostring = function(v) return "N" .. v end }) end end return acc end
P(t2(600))
debug.setmetatable(0, nil)
local function t3(n) local acc = 0 for i = 1, n do acc = acc + #f("%d%s", i, "k") if i == 300 then debug.setmetatable(0, { __index = {} }) end end return acc end
P(t3(600))
debug.setmetatable(0, nil)
-- the function replaced; method form
local function r1(n) local acc = 0 for i = 1, n do if i == 300 then f = function(a, b) return "repl" .. b end end acc = acc + #f("%d", i) end return acc end
P(r1(600))
f = string.format
local function r2(n) local acc = 0 for i = 1, n do acc = acc + #(("%d-%d"):format(i, i + 1)) end return acc end
P(r2(500))
-- traceback and error positions after the C call
local function tb(n) local acc = 0 for i = 1, n do acc = acc + #f("%d", i) end return debug.traceback("tb", 1) end
P((tb(500):gsub("\n.*", "")))
local function er(n) for i = 1, n do local s = f("%d", i < 400 and i or 1.5) end end
local ok, msg = pcall(er, 500) P(ok, msg)
local ok, msg = xpcall(er, function(m) return "H:" .. m .. "|" .. (debug.traceback("", 1):gsub("\n", "/")) end, 500) P(ok, (msg:gsub("0x%x+", "PTR")))
-- coroutines: format inside a wrapped generator, a yield across pcall
local gen = coroutine.wrap(function() for i = 1, 400 do coroutine.yield(f("%03d-%s", i, "w")) end return "done" end)
local gl, gn = "", 0 for i = 1, 401 do gl = gen() gn = gn + #gl end P(gl, gn)
local co = coroutine.create(function() local acc = 0 for i = 1, 300 do local ok, v = pcall(function() coroutine.yield(i) return f("%d:%x", i, i) end) acc = acc + #v end return acc end)
local cs = 0 while true do local ok, v = coroutine.resume(co) if coroutine.status(co) == "dead" then P(ok, v) break end cs = cs + v end P(cs)
-- a count hook set in the middle of a format loop
local hc = 0
local function hk(n) local acc = 0 for i = 1, n do acc = acc + #f("%d", i) if i == 300 then debug.sethook(function() hc = hc + 1 end, "", 50) end end debug.sethook() return acc end
P(hk(900), hc > 0)
-- four and five arguments (the stash op `FMTX` + the extended `FMT`)
local function w5(n) local acc = 0 for i = 1, n do acc = acc + #f("%08x  %s  %-7s %s%s\n", i * 77, "ab", i % 2 == 0 and "mov" or "ldr", "r1, r2", i % 3 == 0 and "" or "x") end return acc end
P(w5(600))
local function w5b(n) local t = {} for i = 1, n do t[#t + 1] = f("%d|%d|%d|%d|%d", i, -i, i * 3, 7, i % 9) end return t[1], t[n], #table.concat(t) end
P(w5b(500))
local function w4(n) local acc = 0 for i = 1, n do acc = acc + #f("%s/%s/%s/%d", "a", i % 2 == 0 and long or "b", "c", i) end return acc end
P(w4(600))
local function w4b(n) local acc = 0 for i = 1, n do local s = f("%c%c%c%c", 65 + i % 26, 66, 67, 68) acc = acc + s:byte(1) + #s end return acc end
P(w4b(600))
local function w5c(n, v) local acc = 0 for i = 1, n do acc = acc + #f("%s %s %s %s %s", "a", "b", "c", i < 300 and "d" or v, "e") end return acc end
P(pcall(w5c, 600, "x")) P(pcall(w5c, 600, 1)) P(pcall(w5c, 600, 1.5)) P(pcall(w5c, 600, nil)) P(pcall(w5c, 600, {}))
local function w5d(n) local acc = 0 for i = 1, n do acc = acc + #f("%d %d %d %d %d", i, i, i, i, i < 300 and i or 2.5) end return acc end
P(pcall(w5d, 600))
local function w5e(n) local acc = 0 for i = 1, n do acc = acc + #f("%d %d %d %d", i, i, i) end return acc end
P(pcall(w5e, 600))
local function w5f(n) local acc = 0 for i = 1, n do acc = acc + #f("%s%s", f("%s-%d-%s-%d", "p", i, "q", i + 1), f("%d,%d,%d,%d", 1, 2, 3, i)) end return acc end
P(w5f(500))
local function w5g(n) local acc = 0 for i = 1, n do local a = f("%d %d %d %d", i, 2, 3, 4) local b = f("%d %d %d %d", i, 2, 3, 4) acc = acc + #a + #b end return acc end
P(w5g(500))
-- result used as a table key (interned short strings) and compared
local keys = {}
local function k1(n) for i = 1, n do local s = f("k%d", i % 17) keys[s] = (keys[s] or 0) + 1 end return keys.k0, keys.k16 end
P(k1(700))
local function k2(n) local c = 0 for i = 1, n do if f("%x", i) == "ff" then c = c + 1 end end return c end
P(k2(700))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "tc_ccall",
        small: true,
        src: r####"
-- BUNDLE-C `t1ccall`: calls of light C functions from tier 1: wanted-result counts 0 / 1 / 2 / 3 / multret, errors, hooks, yields, deep stacks.
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
local find, sub, byte, rep, upper, tostring, tonumber, select, unpack = string.find, string.sub, string.byte, string.rep, string.upper, tostring, tonumber, select, table.unpack
local function w(n)
  local acc = 0
  for i = 1, n do
    find("hello world", "o w")           -- 0 results wanted
    local a = sub("hello", 2, 3)          -- 1
    local b, c = find("hello world", "wor") -- 2
    local d, e, g = byte("abc", 1, 3)     -- 3
    local t = { byte("abcd", 1, -1) }     -- multret
    local h = select("#", find("x", "y")) -- nested multret
    acc = acc + #a + b + c + d + e + g + #t + h + #upper("q") + #rep("z", 3) + (tonumber("12") or 0)
  end
  return acc
end
P(w(2000))
-- errors from the C function unwind to pcall with the right message and level
local function bad(i) return sub() end
for i = 1, 300 do local ok, m = pcall(bad, i) if i == 1 or i == 300 then P(ok, m) end end
local function bad2(i) return rep("x", 1 << 62) end
for i = 1, 300 do local ok, m = pcall(bad2, i) if i == 300 then P(ok, m) end end
local function bad3(i) return ("x"):bad() end
for i = 1, 300 do local ok, m = pcall(bad3, i) if i == 300 then P(ok, m) end end
local function bad4(i) return error("e" .. i, 2) end
for i = 1, 300 do local ok, m = pcall(function() bad4(i) end) if i == 300 then P(ok, m) end end
local function bad5(i) return setmetatable(1, 2) end
for i = 1, 300 do local ok, m = pcall(bad5, i) if i == 300 then P(ok, m) end end
-- xpcall handlers see the C frame: traceback / getinfo of the C function
local function inC(i) return find(nil, "x") end
local function h(m) local inf = debug.getinfo(2, "Sln") return m .. "|" .. tostring(inf.what) .. "|" .. tostring(inf.name) .. "|" .. (debug.traceback("", 2):gsub("\n", "/")) end
for i = 1, 300 do local ok, m = xpcall(inC, h, i) if i == 300 then P(ok, (m:gsub("0x%x+", "PTR"))) end end
-- debug.getinfo of a running C function from a callback
local seen
for i = 1, 300 do table.sort({ 3, 2, 1 }, function(x, y) local inf = debug.getinfo(2, "Sl") seen = inf.what return x < y end) end
P(seen)
for i = 1, 300 do local s = string.gsub("abc", "b", function(x) seen = debug.getinfo(2, "S").what return x end) end
P(seen)
-- debug.traceback inside and after C calls
local function tb1() return debug.traceback("m", 1) end
local r = "" for i = 1, 300 do r = tb1() end P((r:gsub("0x%x+", "PTR"):gsub("\n", "/")))
local function viaC() return select(2, pcall(tb1)) end
for i = 1, 300 do r = viaC() end P((r:gsub("0x%x+", "PTR"):gsub("\n", "/")))
-- C functions that yield (coroutine.yield called from tier 1), wrap, isyieldable
local co = coroutine.wrap(function(...)
  local acc = 0
  for i = 1, 500 do acc = acc + coroutine.yield(i) + (coroutine.isyieldable() and 1 or 0) + (find("abc", "b")) end
  return acc
end)
local v = co() local tot = 0 for i = 1, 500 do tot = tot + v v = co(i) end P(tot, v)
-- yield across pcall in a coroutine that also calls light C functions
local cr = coroutine.create(function() local s = 0 for i = 1, 400 do local ok, x = pcall(function() local y = coroutine.yield(i) return y + #sub("hello", 1, i % 5) end) s = s + x end return s end)
local last = 0 while true do local ok, x = coroutine.resume(cr, 3) if coroutine.status(cr) == "dead" then P(ok, x) break end last = last + x end P(last)
-- error inside a coroutine through a C call, resume reports it; wrap rethrows
local ce = coroutine.create(function() for i = 1, 400 do local x = sub("abc", 1, 1) if i == 399 then sub() end end end)
local ok, m
for i = 1, 500 do ok, m = coroutine.resume(ce) if not ok then break end end P(ok, m)
local cw = coroutine.wrap(function() for i = 1, 400 do rep("x", 2) if i == 350 then error("wrapped") end end end)
P(pcall(function() for i = 1, 500 do cw() end end))
-- a hook installed while the loop runs
local hc = 0
local function hk(n) local acc = 0 for i = 1, n do acc = acc + #sub("hello", 1, 2) if i == 500 then debug.sethook(function() hc = hc + 1 end, "", 20) end end debug.sethook() return acc end
P(hk(1500), hc > 0)
local calls = 0
local function hk2(n) local acc = 0 for i = 1, n do acc = acc + #sub("hello", 1, 2) if i == 300 then debug.sethook(function() calls = calls + 1 end, "c") end end debug.sethook() return acc end
P(hk2(800), calls > 0)
-- the stack must grow in a C call: many arguments, deep recursion into C functions that call back
local function deep(n) if n == 0 then return 0 end return 1 + select(2, pcall(deep, n - 1)) end
P(deep(150))
local big = {} for i = 1, 200 do big[i] = i end
local function sp(n) local acc = 0 for i = 1, n do acc = acc + select("#", unpack(big)) end return acc end
P(sp(300))
local function sp2(n) local acc = 0 for i = 1, n do acc = acc + select("#", rep("a", 2), unpack(big, 1, 190)) end return acc end
P(sp2(300))
-- __call on a table callee and a C closure callee (not light)
local callable = setmetatable({}, { __call = function(self, a) return a end })
local function cc(n) local acc = 0 for i = 1, n do acc = acc + callable(i) end return acc end
P(cc(500))
local function gm2(n) local acc = 0 for i = 1, n do local g = ("a b c"):gmatch("%a") acc = acc + #g() + #g() end return acc end
P(gm2(500))
-- the callee changes between light C functions at the same call site, and between a C and a Lua function
local fs = { string.reverse, upper, function(s) return s .. "!" end, tostring, string.lower }
local function poly(n) local acc = 0 for i = 1, n do acc = acc + #fs[i % 5 + 1]("Hello") end return acc end
P(poly(1000))
-- a C function that returns more results than wanted and fewer
local function res(n) local acc = 0 for i = 1, n do local a = byte("abc", 1, 3) local x, y, z, q = byte("ab", 1, 2) acc = acc + a + (z and 1 or 0) + (q and 1 or 0) end return acc end
P(res(500))
-- to-be-closed variables around C calls
local function tbc(n) local acc = 0 for i = 1, n do local c <close> = setmetatable({}, { __close = function() acc = acc + 1 end }) acc = acc + #sub("abc", 1, 1) end return acc end
P(tbc(400))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "bd_mm_depth",
        small: true,
        src: r####"
-- BUNDLE-D (`ccalls`): recursion through metamethods and pcall at every depth around LUAI_MAXCCALLS, after a hot shallow phase: the C-stack check of an
-- inlined metamethod frame / pcall must raise "C stack overflow" at exactly the depth the interpreter does.
local dm = {}
dm.__index = function(t, k) if k <= 0 then return 0 end return t[k - 1] + 1 end
dm.__add = function(a, b) if a.n <= 0 then return b end return setmetatable({ n = a.n - 1 }, dm) + b end
dm.__unm = function(a) if a.n <= 0 then return 0 end return -setmetatable({ n = a.n - 1 }, dm) - 1 end
dm.__lt = function(a, b) if a.n <= 0 then return true end return setmetatable({ n = a.n - 1 }, dm) < b end
local obj = setmetatable({}, dm)
local function f1(d) return obj[d] end
local function f2(d) return setmetatable({ n = d }, dm) + 1 end
local function f3(d) return -setmetatable({ n = d }, dm) end
local function f4(d) return setmetatable({ n = d }, dm) < 1 end
local function rec(k) if k <= 0 then return 0 end local o, r = pcall(rec, k - 1); if not o then error(r, 0) end return r + 1 end
local fs = { f1, f2, f3, f4, rec }
local function probe(kind, d)
  local ok, v = pcall(fs[kind], d)
  if ok then return true, v end
  return false, (tostring(v):gsub("0x%x+", "P"):sub(1, 60))
end
local acc = 0
for i = 1, 400 do
  local ok, v = probe(i % 5 + 1, i % 9)
  acc = acc + (ok and type(v) == "number" and v or 100)
end
print("shallow", acc)
for kind = 1, 5 do
  local bits, first = {}, nil
  for d = 170, 215 do
    local ok, v = probe(kind, d)
    bits[#bits + 1] = ok and "1" or "0"
    if not ok and not first then first = d .. ":" .. v end
  end
  print(kind, table.concat(bits), first)
end
acc = 0
for i = 1, 400 do
  local ok, v = probe(i % 5 + 1, i % 7)
  acc = acc + (ok and type(v) == "number" and v or 100)
end
print("shallow again", acc)
-- a hot loop whose trace (with an inlined `__index` / `__lt` frame) is entered from C depths around the limit
local hotobj = setmetatable({}, { __index = function(t, k) return k * 2 end })
local lta = setmetatable({ n = 0 }, dm)
local function hot(n)
  local s = 0
  for i = 1, n do
    s = s + hotobj[i % 7]
    if lta < lta then s = s + 1 end
  end
  return s
end
local nest = setmetatable({}, { __index = function(t, k) if k <= 0 then return hot(60) end return t[k - 1] end })
print("hot", hot(300), hot(300))
local bits, first = {}, nil
for d = 170, 215 do
  local ok, v = pcall(function() return nest[d] end)
  bits[#bits + 1] = ok and "1" or "0"
  if not ok and not first then first = d .. ":" .. tostring(v):gsub("0x%x+", "P"):sub(1, 60) end
end
print("nest", table.concat(bits), first)
local bits2 = {}
local function pnest(d) if d <= 0 then return hot(60) end local ok, v = pcall(pnest, d - 1); if not ok then error(v, 0) end return v end
for d = 170, 215 do local ok, v = pcall(pnest, d); bits2[#bits2 + 1] = ok and "1" or "0" end
print("pnest", table.concat(bits2))
-- deep, then hot again at the depth just inside the limit
for kind = 1, 5 do
  local s = 0
  for i = 1, 60 do local ok, v = probe(kind, 150 + i % 3); s = s + (ok and type(v) == "number" and v or 1) end
  print("inside", kind, s)
end
"####,
    },
    Prog {
        name: "bd_arith_mm",
        small: true,
        src: r####"
-- BUNDLE-D (`armm`): tier 1 continues in compiled code after the Lua metamethod of an arithmetic instruction returns. Results of every type, metamethods on
-- one operand only, plain tables / nil / strings / booleans as operands (the interpreter's error messages), a metamethod that errors, the metamethod replaced in
-- the middle of the run, `+ - * / % // & << -x ~x #x ^ ..`, tail-call and plain returns.
local amt = {}
local rv = { 1, true, "cmp" }
local ncall = 0
amt.__add = function(a, b) ncall = ncall + 1
  if rawequal(a, b) and rv[1] == "err" then error("add" .. ncall) end
  if rv[1] == "cmp" then return (type(a) == "table" and a.v or a) + (type(b) == "table" and b.v or b) end return rv[1] end
amt.__sub = function(a, b) ncall = ncall + 1; if rv[2] == "cmp" then return (type(a) == "table" and a.v or a) - (type(b) == "table" and b.v or b) end return rv[2] end
amt.__mul = function(a, b) ncall = ncall + 1; return rv[3] == "cmp" and 2 * (type(a) == "table" and a.v or a) or rv[3] end
amt.__unm = function(a) ncall = ncall + 1; if rv[2] == "cmp" then return -a.v end return rv[3] end
amt.__div = function(a, b) ncall = ncall + 1; return setmetatable({ v = a.v }, amt) end
amt.__mod = function(a, b) return 7 end
local bandv = 5
amt.__band = function(a, b) return bandv end
amt.__idiv = function(a, b) return bandv end
amt.__shl = function(a, b) return 3 end
amt.__bnot = function(a) return "bnot" end
amt.__pow = function(a, b) return 0.5 end
amt.__len = function(a) return 9 end
local function new(v) return setmetatable({ v = v }, amt) end
local plain = {}
local function add(a, b) return a + b end
local function sub(a, b) return a - b end
local function mul(a, b) return a * b end
local function unm(a) return -a end
local function div(a, b) return a / b end
local function chain(a, b, c) return a + b * c - a end
local function run(n, A)
  local objs = { new(1), new(2), new(-3), plain, 5, 2.5, "s", "10", false, new(0) }
  local acc, log = 0, {}
  local function mix(v) acc = (acc * 31 + (type(v) == "number" and math.floor(v * 7) % 100003 or #tostring(v))) % 1000000007 end
  for i = 1, n do
    local a, b = objs[i % #objs + 1], objs[(i * A) % #objs + 1]
    if i == n // 3 then rv[1], rv[2], rv[3] = false, "x", "cmp" end
    if i == n // 2 then amt.__add = nil end
    if i == (n * 2) // 3 then amt.__add = function(x, y) return 4 end; rv[1] = "cmp" end
    for _, f in ipairs{ add, sub, mul, unm, div, chain } do
      local ok, r = pcall(f, a, b, objs[(i + 1) % #objs + 1])
      mix(ok)
      if ok then mix(type(r) == "table" and r.v or r) else
        local m = tostring(r):gsub("0x%x+", "P"):gsub("^[^:]*:%d+: ", "")
        if #log < 40 then log[#log + 1] = m end
        mix(m)
      end
    end
    if i % 5 == 0 then
      local ok, r = pcall(function() return a % b, a // b, a & b, a << 1, ~a, #a, a ^ 2 end)
      mix(ok); if ok then mix(r) else mix(tostring(r):gsub("0x%x+", "P")) end
    end
    local x = objs[1]
    local _, y = pcall(add, x, objs[2])
    local _, z = pcall(unm, x)
    mix(y); mix(z)
    -- the result of a bitwise / integer-division metamethod feeds typed arithmetic (the destination register is typed Int after the fast path)
    bandv = ({ 5, "7", 2.5, plain, false, 11, "x" })[i % 7 + 1]
    local ok, q, r = pcall(function() local r = x & 3; local q = r + 1; local w = r * 2 + q; return q + w, r end)
    mix(ok); if ok then mix(q); mix(r) else mix(tostring(q):gsub("0x%x+", "P"):gsub("^[^:]*:%d+: ", "")) end
    ok, q, r = pcall(function() local r = x // 2; local q = r - 1; return q * 3, r end)
    mix(ok); if ok then mix(q); mix(r) else mix(tostring(q):gsub("0x%x+", "P"):gsub("^[^:]*:%d+: ", "")) end
  end
  return acc, ncall, table.concat(log, "|"):sub(1, 400)
end
print(run(500, 3))
print(run(400, 7))
-- errors inside the metamethod, caught by pcall, in a loop
local em = { __add = function(a, b) if a.v % 3 == 0 then error("boom" .. a.v) end return a.v + b end }
local function errs(n)
  local s, e = 0, 0
  for i = 1, n do
    local o = setmetatable({ v = i }, em)
    local ok, r = pcall(function() local r = o + 1; return r end)
    if ok then s = s + r else e = e + 1 end
  end
  return s, e
end
print(errs(600))
-- yields inside the metamethods
local ym = {}
ym.__add = function(a, b) local r = coroutine.yield(a.v); return r end
ym.__unm = function(a) coroutine.yield("unm"); return a.v end
ym.__sub = function(a, b) if b == 0 then error("sub0") end coroutine.yield("sub"); return a.v - b end
local function y(v) return setmetatable({ v = v }, ym) end
local fin
local function body(n)
  local p, q = y(1), y(2)
  local acc = 0
  for i = 1, n do
    local s = p + q
    if type(s) == "number" then acc = acc + s end
    acc = acc + (-p)
    acc = acc + (p - 3)
    local ok, e = pcall(function() return p - 0 end)
    if not ok then acc = acc + #e end
  end
  fin = acc
  return acc
end
for _, m in ipairs{ 2, 3, 5 } do
  fin = nil
  local co = coroutine.wrap(function() return body(80) end)
  local steps, v = 0, co()
  while not fin do steps = steps + 1; v = co(steps % m) end
  print("yield", m, steps, fin)
end
"####,
    },
    Prog {
        name: "bd_vec_loop",
        small: true,
        src: r####"
-- BUNDLE-D (`armm`): vectors with `__add` / `__sub` / `__mul` / `__unm` / `__div` chains in loops (constructors returned by tail call and by plain return),
-- a metamethod that is a vararg function, a table-valued result stored in the destination register, recursion through `__add` at several depths.
local vt = {}
local function vec(x, y) return setmetatable({ x = x, y = y }, vt) end
vt.__add = function(a, b) return vec(a.x + b.x, a.y + b.y) end
vt.__sub = function(a, b) local r = vec(a.x - b.x, a.y - b.y); return r end
vt.__mul = function(a, b)
  if type(a) == "number" then return vec(a * b.x, a * b.y) elseif type(b) == "number" then return vec(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
vt.__unm = function(a) return vec(-a.x, -a.y) end
vt.__div = function(a, b) local x, y = a.x / b, a.y / b; return vec(x, y) end
vt.__len = function(a) return 2 end
local function run(n, K, M)
  local p, v = vec(0, 0), vec(1, K)
  local acc = 0
  for i = 1, n do
    p = p + v * 0.5 - (-v) / 4
    local d = p - v
    acc = acc + d * d + #p
    if i % M == 0 then p = vec(i % 7, 0); v = vec(1, i % 3) end
    if i == n // 2 then vt.__mul = function(a, b) if type(a) == "number" or type(b) == "number" then return vec(1, 1) end return 1 end end
  end
  return acc, p.x, p.y
end
print(run(600, 1, 13))
vt.__mul = function(a, b)
  if type(a) == "number" then return vec(a * b.x, a * b.y) elseif type(b) == "number" then return vec(a.x * b, a.y * b) end
  return a.x * b.x + a.y * b.y
end
print(run(500, 3, 50))
-- vararg metamethod, and a result stored over an operand register
local vm = { __add = function(...) local a, b = ...; return select("#", ...) + (type(a) == "table" and a.v or a) + (type(b) == "table" and b.v or b) end }
local function sum(n)
  local x, y = setmetatable({ v = 1 }, vm), setmetatable({ v = 2 }, vm)
  local s = 0
  for i = 1, n do
    x = x + y
    y = y + x
    s = s + 1
    if type(x) == "number" then x = setmetatable({ v = x % 100 }, vm) end
    if type(y) == "number" then y = setmetatable({ v = y % 100 }, vm) end
  end
  return s, x.v, y.v
end
print(sum(700))
-- recursion through __add
local rm = {}
rm.__add = function(a, b) if a.n <= 0 then return b end return setmetatable({ n = a.n - 1 }, rm) + b end
local function deep(d) return (setmetatable({ n = d }, rm) + 5) end
local out = {}
for i = 1, 300 do out[#out + 1] = deep(i % 12) end
print(table.concat(out, ",", 1, 20), #out)
for _, d in ipairs{ 20, 50, 100, 150, 190 } do
  local ok, v = pcall(deep, d)
  print(d, ok, ok and v or (tostring(v):gsub("0x%x+", "P"):sub(1, 50)))
end
"####,
    },
    Prog {
        name: "tc_fmt_hot",
        small: true,
        src: r####"
-- BUNDLE-C `tfmt`: a hot loop of recorded formats (a trace that runs without leaving)
local f = string.format
local acc = 0
for i = 1, 6000 do
  local s = f("%08x|%-6d|%5s", i * 31, i, "ab")
  local t = f("%d:%s", i, s)
  acc = acc + #s + #t
end
print(acc)
"####,
    },
    Prog {
        name: "rawfn_loops",
        small: true,
        src: r####"
-- C43 runner survivors: `rawget(t, "str")` of a key that is absent when the side trace records and appears later (the absence proof must be a
-- guard), and the result of `rawset(t, k, v)` (the table itself) used by the loop
local P = setmetatable({ a = 1, b = false }, { __index = function() return "idx" end })
local keys = { "a", "b", "c", "d" }
local function rg(n)
  local s = 0
  for i = 1, n do
    local v = rawget(P, keys[i % 4 + 1])
    if v == nil then s = s + 1 elseif v == false then s = s + 10 else s = s + v * 100 end
    if i == n // 2 then P.c = 5 end
  end
  return s
end
print(rg(4000))
P.c = nil
print(rg(4000))
local T = { 1, 2, 3, 4 }
local acc = 0
for r = 1, 2000 do
  for k = 1, 4 do
    local u = rawset(T, k, r + k)
    if u == T then acc = acc + 1 end
    acc = acc + u[k]
  end
end
print(acc)
"####,
    },
    Prog {
        name: "bf_stitch_chain",
        small: true,
        src: r####"
-- BUNDLE-F `stitch0`: a loop with a C call whose side traces start at the CALL (a side recording that stitches before its
-- first bytecode is an empty trace; chains of them). Several C functions, a branch that flips, a call through a local.
local f = math.ult
local g = string.byte
local t = {1, 2, 3, 4, 5, 6, 7, 8}
local function run(n, mode)
  local a, b, c, d = 0, 1, 2, 3
  for i = 1, n do
    a = (a + t[1] * i) & 0xffff; b = (b + t[2] + a) & 0xffff; c = (c + (a ~ b)) & 0xffff; d = (d + t[3] + c) & 0xffff
    a = (a + t[4] * i) & 0xffff; b = (b + t[5] + a) & 0xffff; c = (c + (a ~ b)) & 0xffff; d = (d + t[6] + c) & 0xffff
    if f(i, 5) then a = a + 1 end
    if mode == 1 or i % 3 == 0 then d = d + (g("abc", i % 3 + 1) or 0) end
    if i % 7 == 0 then c = c + select("#", i, a, b) end
    if i % 1000 == 999 then pcall(error, "x") end
  end
  return a + b + c + d
end
print(run(3000, 0), run(3000, 1), run(300, 0))
"####,
    },
    Prog {
        name: "bf_rec",
        small: true,
        src: r####"
-- BUNDLE-F `fibdiet`: native recursion. Tree recursion, pcall around the recursion, a coroutine inside, a hook mid-run.
local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
local s = 0
for i = 1, 6 do s = s + fib(14 + i % 3) end
print("fib", s)
local function down(n) if n == 0 then return 0 end return 1 + down(n - 1) end
for r = 1, 40 do
  local ok, v = pcall(down, 150 + r * 20)
  s = s + (ok and v or -1)
end
print("down", s)
local function cofib(n)
  if n < 2 then if n == 1 then coroutine.yield(1) end return n end
  return cofib(n - 1) + cofib(n - 2)
end
local co = coroutine.wrap(function() return cofib(12) end)
local ys = 0
while true do local v = co(); if v == 1 and ys < 1000 then ys = ys + 1 else s = s + v; break end end
print("co", ys, s)
local cnt = 0
debug.sethook(function() cnt = cnt + 1 end, "", 997)
for i = 1, 4 do s = s + fib(13) end
debug.sethook()
print("hook", cnt > 0, s)
local function perm(n, t) if n == 0 then return 1 end local c = 0 for i = 1, 3 do c = c + perm(n - 1, t) end return c end
print(perm(7, {}), perm(6, {}))
"####,
    },
    Prog {
        name: "bf_rec_deep",
        small: true,
        src: r####"
-- BUNDLE-F `fibdiet`: recursion to the stack-overflow limit, three times (the depth reached and the error must equal the interpreter's),
-- a call chain of a second shape (two live locals) and one from inside a pcall'd coroutine.
local depth
local function deep(n) depth = n; return 1 + deep(n + 1) end
for r = 1, 3 do
  local ok, err = pcall(deep, 1)
  print("deep", ok, depth, tostring(err):gsub("^.-:%d+: ", ""))
end
local d2
local function deep2(n, a, b) d2 = n; local x, y = a + 1, b + 2; return deep2(n + 1, x, y) + x + y end
local ok, err = pcall(deep2, 1, 0, 0)
print("deep2", ok, d2, tostring(err):gsub("^.-:%d+: ", ""))
local co = coroutine.create(function() return deep(1) end)
local ok2, err2 = coroutine.resume(co)
print("co", ok2, depth, tostring(err2):gsub("^.-:%d+: ", ""))
"####,
    },
    Prog {
        name: "rand_storm",
        small: true,
        src: r####"
-- C43: three recorded `math.random` draws per iteration; under `+exitstorm=N` the guard behind a draw must not fail spuriously
-- (the generator has advanced: the interpreter's re-executed CALL would draw again and every sum after it is off)
math.randomseed(12345)
local rs1, rs2, rs4 = 0, 0, 0
for r = 1, 400 do
  for i = 1, 20 do
    rs1 = rs1 + math.random(1, 100)
    rs2 = rs2 + math.random(i + 5)
    rs4 = rs4 + math.random(i, i)
  end
end
print(rs1, rs2, rs4, math.random(1000))
"####,
    },
    Prog {
        name: "bh_lib",
        small: true,
        src: r####"
-- BUNDLE-H `libcall`: n-ary string.char, math.random(), string.lower / upper / reverse / rep, type(), string compares, in loops that change shape
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
-- string.char with two and three arguments; the codes leave the range in the middle of the loop (an error through pcall)
local function c2(n, bad) local acc, last = 0, "" for i = 1, n do local s = string.char(i % 256, (i * 7) % 256) acc = acc + s:byte(1) * 3 + s:byte(2) last = s if bad and i == n - 3 then string.char(300, 1) end end return acc, #last end
P(c2(900)) P(pcall(c2, 900, true))
local function c3(n) local acc = 0 for i = 1, n do local s = string.char(i % 251, 0, (i * 13) % 256) acc = acc + s:byte(1) + 2 * s:byte(2) + 3 * s:byte(3) + #s end return acc end
P(c3(900))
local function c3x(n) local t = {} for i = 1, n do t[#t + 1] = string.char(65 + i % 26, 128 + i % 100, 255 - i % 3) end return table.concat(t):sub(1, 30), #table.concat(t) end
P(c3x(500))
P(pcall(string.char, 65, 66, 256)) P(pcall(string.char, 65, -1)) P(pcall(string.char, 1, 2, 3, 4, 5))
-- math.random(): seeded, so the sequence is the reference's
math.randomseed(1234, 5)
local function rf(n) local acc, mx, mn = 0, 0, 1 for i = 1, n do local x = math.random() acc = acc + x if x > mx then mx = x end if x < mn then mn = x end end return acc, mx, mn end
P(rf(1500))
math.randomseed(77)
local function rf2(n) local t = {} for i = 1, n do t[#t + 1] = string.format("%.17g", math.random() * 1000) end return t[1], t[#t], #t end
P(rf2(400))
math.randomseed(9)
local function rm(n, m) local acc = 0 for i = 1, n do acc = acc + math.random(m) + math.random(2, m + 2) end return acc end
P(rm(1000, 6)) P(rm(500, 100000))
-- a random() that is not math.random's (another closure replaced it in the middle of the loop)
local real = math.random
local function swap(n) local acc = 0 for i = 1, n do if i == n // 2 then math.random = function() return 0.5 end end acc = acc + math.random() end return acc end
math.randomseed(3)
P(swap(1000)) math.random = real
math.randomseed(3) P(real(), real(), real())
-- lower / upper / reverse: short strings, long strings, the class changes in the middle, embedded zeros, high bytes
local words = { "Hello", "WORLD", "mIxEd", "", "a\0B\0c", "\200\201Zz", ("Long String "):rep(5), ("x"):rep(41), ("Y"):rep(40) }
local function lu(n) local acc = 0 for i = 1, n do local w = words[i % #words + 1] local a, b, c = w:lower(), w:upper(), w:reverse() acc = acc + #a + (a:byte(1 + #a // 2) or 0) * 2 + (b:byte(1) or 0) + (c:byte(-1) or 0) end return acc end
P(lu(900))
local function lu2(n) local t = {} for i = 1, n do local w = string.lower(words[i % 6 + 1]) .. string.upper(words[(i + 2) % 6 + 1]) .. string.reverse(words[(i + 4) % 6 + 1]) t[#t + 1] = #w end return table.concat(t, "", 1, 12), #t end
P(lu2(600))
P(pcall(string.lower)) P(pcall(string.upper, {})) P(pcall(string.reverse, nil)) P(("abc"):lower(1, 2, 3))
-- number arguments convert (a string argument that is a number)
local function ln(n) local acc = 0 for i = 1, n do acc = acc + #(tostring(i)):reverse() + tonumber((i .. ""):upper()) end return acc end
P(ln(500))
-- string.rep: counts of every sign, separators, empty strings, the result class changes
local function rp(n) local acc = 0 for i = 1, n do acc = acc + #string.rep("ab", i % 30) + #string.rep("x", i % 7 - 2) + #string.rep("", i) + #string.rep("k", i % 9, ",") end return acc end
P(rp(900))
local function rp2(n) local t = {} for i = 1, n do t[#t + 1] = ("ab"):rep(i % 25 + 1, "--") end return t[1], t[#t], #table.concat(t) end
P(rp2(400))
P(pcall(string.rep)) P(pcall(string.rep, "x")) P(pcall(string.rep, "x", {})) P(pcall(string.rep, "x", 1 << 62)) P(pcall(string.rep, "", 1 << 62)) P(#string.rep("", 1 << 40, ""), pcall(string.rep, "ab", math.maxinteger, ","))
P(#string.rep("abc", 0), #string.rep("abc", -3), string.rep("abc", 1, "-"), string.rep("abc", 2, ""))
-- BUNDLE-R: the result class computed from the lengths: a count that walks across the 40-byte limit with and without a separator, the argument
-- lengths changing (the empty string, one byte, a long string), single-byte units, counts of every sign, a count above the helper's cap
local rs = { "", "x", "ab", "abc", ("q"):rep(41), ("w"):rep(20) }
local sp = { "", ",", ", ", ("-"):rep(41) }
local function rpc(n) local t, long = {}, 0 for i = 1, n do
  local s, c, p = rs[i % 6 + 1], (i * 7) % 47 - 3, sp[i % 4 + 1]
  local a, b = string.rep(s, c), string.rep(s, c, p)
  long = long + #a * 3 + #b + (#b > 40 and 1 or 0)
  t[#t + 1] = a:sub(-3) .. "|" .. b:sub(1, 5) end return long, t[#t], t[n // 2], #t end
P(rpc(1500))
local function rpe(n) local acc = 0 for i = 1, n do acc = acc + #string.rep("x", 39 + i % 3) + #string.rep("x", 20, "y") + #string.rep("ab", 20 + i % 2) + #string.rep("", i % 50, ",") end return acc end
P(rpe(800))
local function rbig(n) local acc = 0 for i = 1, n do acc = acc + #string.rep("x", i == n // 2 and 3000000 or 3 + i % 5) end return acc end
P(rbig(600))
P(#string.rep("x", 4194304), #string.rep("x", 4194305), #string.rep("ab", 2097152), #string.rep("", 4194305, ","))
local function rneg(n) local acc = 0 for i = 1, n do acc = acc + #string.rep("abc", 5 - i % 12, ".") + #string.rep("abc", i % 3 == 0 and -i or i % 20) end return acc end
P(rneg(700))
P(pcall(function() for i = 1, 600 do local x = string.rep("ab", i == 500 and 1 << 62 or 4) end end))
P(pcall(function() for i = 1, 600 do local x = string.rep("ab", i == 500 and 2.5 or 4) end end))
P(pcall(function() for i = 1, 600 do local x = string.rep("ab", i == 500 and 3.0 or 4) end end))
-- type(): all kinds, the callee replaced in the middle of the loop
local tv = { 1, 1.5, "s", true, nil, {}, print, function() end, coroutine.create(function() end), io.stdout }
local function ty(n) local t = {} for i = 1, n do t[#t + 1] = type(tv[i % 10 + 1]) end return table.concat(t, ",", 1, 10), #t end
P(ty(700))
local function ty2(n) local acc = 0 for i = 1, n do if i == n // 2 then type = function(x) return "fake" end end acc = acc + #type(i) end return acc end
local realtype = type
P(ty2(800)) type = realtype
-- the callee replaced by another C closure (the helper must notice it is not `type`)
local function ty3(n) local acc = 0 for i = 1, n do if i == n // 2 then type = string.gmatch(("a"):rep(2000), "a") end acc = acc + #type(i) end return acc end
P(pcall(ty3, 800)) type = realtype
P(math.type(1), math.type(1.5), math.type("1"), math.type(nil))
-- string compares: short, long, equal prefixes, embedded zeros, the length class changes
local sv = { "a", "b", "ab", "abc", "", "a\0b", "a\0", ("p"):rep(50), ("p"):rep(50) .. "q", ("p"):rep(49) .. "q", "\255", "\200a" }
local function cm(n) local lt, le, gt = 0, 0, 0 for i = 1, n do local a, b = sv[i % 12 + 1], sv[(i * 7) % 12 + 1] if a < b then lt = lt + 1 end if a <= b then le = le + 1 end if a > b then gt = gt + 1 end end return lt, le, gt end
P(cm(1200))
P(pcall(function() return "a" < 1 end)) P(pcall(function() return {} <= "a" end))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "bh_char_hot",
        small: true,
        src: r####"
-- BUNDLE-H: a hot loop of three-argument string.char
local acc = 0
for i = 1, 6000 do
  local s = string.char(65 + i % 26, 66, 97 + i % 20)
  acc = acc + #s + s:byte(3)
end
print(acc)
"####,
    },
    Prog {
        name: "bh_rand_hot",
        small: true,
        src: r####"
-- BUNDLE-H: a hot loop of math.random()
math.randomseed(5)
local acc = 0
for i = 1, 6000 do acc = acc + math.random() end
print(string.format("%.10f", acc))
"####,
    },
    Prog {
        name: "bh_case_hot",
        small: true,
        src: r####"
-- BUNDLE-H: a hot loop of lower / upper / reverse
local S = {}
for k = 1, 16 do S[k] = "the Quick brown FOX " .. k end
local acc = 0
for i = 1, 6000 do
  local s = S[i % 16 + 1]
  acc = acc + #s:lower() + #s:upper() + s:reverse():byte(1)
end
print(acc)
"####,
    },
    Prog {
        name: "bh_rep_hot",
        small: true,
        src: r####"
-- BUNDLE-H: a hot loop of string.rep with a short result
local acc = 0
for i = 1, 6000 do
  acc = acc + #string.rep("ab", 3 + i % 4) + #string.rep("x", 2, ",")
end
print(acc)
"####,
    },
    Prog {
        name: "bh_rep_mixed_hot",
        small: true,
        src: r####"
-- BUNDLE-R: string.rep whose result is short in some iterations and long in others (the class is a guard on the computed length)
local acc = 0
local T = { "ab", "abc", "x" }
for i = 1, 6000 do
  acc = acc + #string.rep(T[i % 3 + 1], 15 + i % 11) + #string.rep("x", 2 + i % 30, ",")
end
print(acc)
"####,
    },
    Prog {
        name: "bh_next",
        small: true,
        src: r####"
-- BUNDLE-H `tnextinl`: traversal loops (pairs / next) over arrays, hash parts, mixed and empty tables, with the table modified during the traversal
-- (assigning nil to or overwriting existing fields is allowed), the table changing between runs of the loop, and `next` driven by hand
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = tostring(t[i]) end out[#out + 1] = table.concat(t, ",") end
local function sum(t) local s, n = 0, 0 for k, v in pairs(t) do s = s + v n = n + 1 end return s, n end
local arr, hash, mixed, empty = {}, {}, {}, {}
for i = 1, 40 do arr[i] = i end
for i = 1, 40 do hash["k" .. i] = i * 2 end
for i = 1, 20 do mixed[i] = i mixed["m" .. i] = -i end
local tabs = { arr, hash, mixed, empty, { 5 }, { x = 1 }, { 1, 2, 3, x = 4, y = 5 } }
local function rounds(n) local s, c = 0, 0 for i = 1, n do local a, b = sum(tabs[i % #tabs + 1]) s = s + a c = c + b end return s, c end
P(rounds(2000))
-- an empty table every time
local function emp(n) local c = 0 for i = 1, n do for k, v in pairs(empty) do c = c + 1 end for k, v in next, empty do c = c + 1 end end return c end
P(emp(3000))
-- next(t) / next(t, k) by hand
local function nx(t, n) local c, s = 0, 0 for i = 1, n do local k, v = next(t) while k ~= nil do c = c + 1 s = s + v k, v = next(t, k) end end return c, s end
P(nx(arr, 100)) P(nx(hash, 100)) P(nx(mixed, 100)) P(nx(empty, 500))
-- assigning nil to the current key, and overwriting existing fields, while traversing
local function clear(n) local total = 0 for i = 1, n do local t = {} for j = 1, 12 do t[j] = j t["s" .. j] = j end local c = 0 for k, v in pairs(t) do t[k] = nil c = c + 1 end for k in pairs(t) do c = c + 1000 end total = total + c end return total end
P(clear(300))
local function over(n) local total = 0 for i = 1, n do local t = {} for j = 1, 10 do t[j] = j t["s" .. j] = j end for k, v in pairs(t) do t[k] = v * 2 end for k, v in pairs(t) do total = total + v end end return total end
P(over(300))
-- removing other (later) fields during the traversal: they must not be visited
local function later(n) local seen = 0 for i = 1, n do local t = { a = 1, b = 2, c = 3, d = 4, 1, 2, 3, 4 } for k, v in pairs(t) do seen = seen + 1 if k == 1 then t[3] = nil t.c = nil end end end return seen end
P(later(200))
-- the table changes shape between traversals (growth, shrink, rehash), the loop itself stays the same
local grow = {}
local function g(n) local c = 0 for i = 1, n do grow[i] = i grow["g" .. i] = i if i % 7 == 0 then grow[i - 3] = nil end for k, v in pairs(grow) do c = c + 1 end end return c end
P(g(120))
-- a metatable with __pairs, and a table with a __index (pairs / next ignore __index)
local mp = setmetatable({}, { __pairs = function(t) return function(_, k) if not k then return 1, "one" elseif k < 3 then return k + 1, "more" end end, t, nil end, __index = function() return 9 end })
local function mpairs(n) local c = 0 for i = 1, n do for k, v in pairs(mp) do c = c + k end end return c end
P(mpairs(300))
-- float keys, boolean keys, negative integer keys, nested traversals
local weird = { [1.5] = "a", [true] = "b", [-3] = "c", [false] = "d", [2^60] = "e", [0] = "z" }
local function wd(n) local c = 0 for i = 1, n do for k, v in pairs(weird) do c = c + #v end end return c end
P(wd(400))
local function nested(n) local c = 0 for i = 1, n do for k, v in pairs(arr) do for k2, v2 in pairs(tabs[5]) do c = c + v + v2 end end end return c end
P(nested(60))
-- an error inside the traversal, and a modification that makes `next` fail ("invalid key to 'next'")
P(pcall(function() local t = { 1, 2, 3 } for k in pairs(t) do error("boom " .. k) end end))
P(pcall(next, { 1, 2 }, "nokey"))
-- a table that dies and is recreated each round
local function fresh(n) local c = 0 for i = 1, n do local t = { i, i + 1, p = i } for k, v in pairs(t) do c = c + v end end return c end
P(fresh(2500))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "bh_next_hot",
        small: true,
        src: r####"
-- BUNDLE-H: a hot traversal of a table
local t = {}
for i = 1, 64 do t[i] = i end
local acc = 0
for r = 1, 400 do
  for k, v in pairs(t) do acc = acc + v end
end
print(acc)
"####,
    },
    Prog {
        name: "j1_sort_objects",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): Lua functions called from C. A sort whose comparator compares objects through Lua `__lt` metamethods
-- (a polymorphic method per class), three sorts with different orders, a stable result printed in full.
local Shape = {}
Shape.__index = Shape
local Sq, Ci, Re = setmetatable({}, {__index = Shape}), setmetatable({}, {__index = Shape}), setmetatable({}, {__index = Shape})
Sq.__index, Ci.__index, Re.__index = Sq, Ci, Re
function Sq:area() return self.w * self.w end
function Ci:area() return 3 * self.w * self.w end
function Re:area() return self.w * self.h end
Shape.__lt = function(a, b) return a:area() < b:area() end
Sq.__lt, Ci.__lt, Re.__lt = Shape.__lt, Shape.__lt, Shape.__lt
local seed = 7
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n + 1 end
local function mk(i)
  local k = i % 3
  if k == 0 then return setmetatable({id = i, w = rnd(9)}, Sq)
  elseif k == 1 then return setmetatable({id = i, w = rnd(9)}, Ci)
  else return setmetatable({id = i, w = rnd(9), h = rnd(9)}, Re) end
end
for round = 1, 6 do
  local t = {}
  for i = 1, 40 + round * 7 do t[i] = mk(i) end
  table.sort(t, function(a, b)
    if a < b then return true end
    if b < a then return false end
    return a.id < b.id
  end)
  local ids = {}
  for i = 1, #t do ids[#ids + 1] = t[i].id end
  print(round, table.concat(ids, ","))
  table.sort(t, function(a, b) return a.id > b.id end)
  print(t[1].id, t[#t].id)
  local c = 0
  table.sort(t, function(a, b) c = c + 1 return a:area() > b:area() or (a:area() == b:area() and a.id < b.id) end)
  print(c > 0, t[1].id, t[#t].id)
end
"####,
    },
    Prog {
        name: "j1_callbacks",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): gsub / gmatch callbacks, table-valued replacements, pcall / xpcall / select / next-driven functions, metamethods called by
-- library functions (tostring's __tostring, __index function through rawget-free paths, __concat via table.concat is not a metamethod),
-- a callback that returns different types.
local words = {}
for i = 1, 30 do words[i] = "w" .. (i * 37 % 11) end
local text = table.concat(words, " ")
local seen = {}
local function cb(w) seen[w] = (seen[w] or 0) + 1 if #w > 2 then return w:upper() end return nil end
for round = 1, 12 do
  local r, n = text:gsub("%a%d+", cb)
  if round % 4 == 0 then print(r, n) end
end
local keys = {}
for k in pairs(seen) do keys[#keys + 1] = k end
table.sort(keys)
for _, k in ipairs(keys) do io.write(k, "=", seen[k], " ") end
print()
local map = {w1 = "ONE", w2 = 2, w3 = false}
for round = 1, 8 do local r = text:gsub("%a%d+", map) if round == 8 then print(r) end end
local function ret3(w) if #w == 2 then return 1 elseif #w == 3 then return 2.5 else return w .. "!" end end
for round = 1, 8 do local r = text:gsub("%a%d+", ret3) if round == 8 then print(r) end end
local acc = 0
for round = 1, 50 do
  for w in text:gmatch("%a%d+") do acc = acc + #w end
end
print(acc)
local T = setmetatable({}, {__tostring = function(t) return "T" .. #t end})
local s = 0
for i = 1, 60 do T[#T + 1] = i; s = s + #tostring(T) end
print(s)
local function f(a, b) return a + b, a - b end
for i = 1, 5 do print(pcall(f, i, 2), select("#", pcall(f, i, 2))) end
local function lvl(n) if n > 3 then error("deep " .. n, 2) end return n end
for i = 1, 6 do print(pcall(lvl, i)) end
print(xpcall(function(a) return a * 2 end, print, 21))
local mt = setmetatable({}, {__index = function(t, k) return k .. k end, __call = function(self, a) return a + 1 end})
local r = 0
for i = 1, 40 do r = r + #mt["ab" .. i % 3] + mt(i) end
print(r, pcall(mt, 5))
"####,
    },
    Prog {
        name: "j1_errors",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): errors, tracebacks and non-local exits through Lua functions that C calls: an error in a sort comparator
-- (an inconsistent order function, a type error, an error object) caught by pcall, debug.traceback / error levels inside callbacks,
-- gsub callbacks that raise, and the state after (the sort of a partly sorted table).
local function try(f, ...) local ok, e = pcall(f, ...) return ok, type(e) == "string" and e:gsub("^[^:]*:%d+: ", "") or tostring(type(e)) end
local t = {}
for i = 1, 70 do t[i] = (i * 7919) % 101 end
for round = 1, 12 do
  local n = 0
  print(round, try(table.sort, t, function(a, b) n = n + 1 if n == 30 + round then error("stop at " .. n) end return a < b end))
end
local u = {}
for i = 1, 70 do u[i] = i % 5 end
print(try(table.sort, u, function(a, b) return true end))
print(try(table.sort, u, function(a, b) return a.x < b end))
print(try(table.sort, {3, 2, 1, "x"}, function(a, b) return a < b end))
local function lines(tb) local o = {} for l in tb:gmatch("[^\n]+") do o[#o + 1] = l end return o end
for round = 1, 3 do
  table.sort(t, function(a, b)
    if round == 3 and a == 50 then
      local tb = debug.traceback("tb", 1)
      local ls = lines(tb)
      print(#ls > 2, ls[1], (ls[2]:gsub("^%s+", "")))
    end
    return a < b
  end)
end
local function raiser(w) if w == "w3" then error({code = 42}) end return w end
for round = 1, 10 do
  local ok, e = pcall(string.gsub, "w1 w2 w3 w4", "%a%d", raiser)
  if round == 10 then print(ok, type(e) == "table" and e.code) end
end
local co = coroutine.wrap(function()
  local ok, e = pcall(table.sort, {3, 1, 2}, function(a, b) coroutine.yield("y") return a < b end)
  return ok, (tostring(e):gsub("^[^:]*:%d+: ", ""))
end)
print(co())
local co2 = coroutine.wrap(function()
  for i = 1, 30 do
    local x = {5, 3, 9, 1, i}
    table.sort(x, function(a, b) return a < b end)
    coroutine.yield(x[1] + x[5])
  end
  return "done"
end)
local sum = 0
for i = 1, 30 do sum = sum + co2() end
print(sum, co2())
local ok, e = pcall(table.sort, {1, 2, 3}, function(a, b) error(setmetatable({}, {__tostring = function() return "custom" end})) end)
print(ok, tostring(e))
"####,
    },
    Prog {
        name: "j1_gc_hooks",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): the collector running inside Lua functions that C calls (allocating comparators and callbacks that also call
-- collectgarbage), a call hook counting the calls of the callbacks, a function that is replaced (reassigned upvalue) between sorts.
local function alloc(a, b) local s = {a, b, tostring(a) .. tostring(b)} return #s[3] % 2 == 0, s end
local t = {}
for i = 1, 60 do t[i] = (i * 37) % 53 end
for round = 1, 10 do
  local cnt = 0
  table.sort(t, function(a, b)
    cnt = cnt + 1
    local x = {a = a, b = b, s = a .. ":" .. b}
    if cnt % 40 == 0 then collectgarbage("step", 1) end
    if cnt % 150 == 0 then collectgarbage() end
    return a < b and #x.s > 0
  end)
  if round % 3 == 0 then print(round, cnt > 0, t[1], t[60]) end
end
local calls = 0
local cmp
local function mkcmp(sign) return function(a, b) if sign > 0 then return a < b else return a > b end end end
for round = 1, 10 do
  cmp = mkcmp(round % 2 == 0 and 1 or -1)
  table.sort(t, cmp)
  if round % 4 == 0 then print(t[1], t[60]) end
end
debug.sethook(function(ev) calls = calls + 1 end, "c")
for round = 1, 3 do table.sort(t, function(a, b) return a < b end) end
debug.sethook()
print(calls > 0, calls)
local lines = 0
debug.sethook(function(ev, l) lines = lines + 1 end, "l")
table.sort(t, function(a, b)
  return a > b
end)
debug.sethook()
print(lines > 0, lines)
local s = 0
for i = 1, 200 do local r = ("x"):rep(i % 7):gsub("x", function(c) return c .. c end) s = s + #r end
print(s)
local weak = setmetatable({}, {__mode = "v"})
for i = 1, 30 do
  table.sort(t, function(a, b) weak[#weak + 1] = {a, b} return a < b end)
end
collectgarbage()
print(#weak >= 0)
"####,
    },
    Prog {
        name: "j1_small_cmp",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): the smallest comparator (`a > b`, fewer bytecodes than `fnmin`, no CALL) called from `table.sort` many times.
local seed = 3
local function rnd() seed = (seed * 16807) % 2147483647 return seed end
local acc = 0
for round = 1, 30 do
  local t = {}
  for j = 1, 60 do t[j] = rnd() % 1000 end
  table.sort(t, function(a, b) return a > b end)
  acc = acc + t[1] % 7 + t[60] % 5
end
print(acc)
"####,
    },
    Prog {
        name: "j1_loop_cmp",
        small: true,
        src: r####"
-- BUNDLE-J (`centry`): a comparator that creates closures (a `CLOSURE` is not recorded: `Nyi(Closure)` aborts every recording of its function-entry head):
-- the head is blacklisted and the calls from C must go back to the interpreter.
local seed = 5
local function rnd() seed = (seed * 16807) % 2147483647 return seed end
local acc = 0
for round = 1, 40 do
  local t = {}
  for j = 1, 60 do t[j] = rnd() % 1000 end
  table.sort(t, function(a, b)
    local f = function() return a end
    local g = function() return b end
    return f() < g()
  end)
  acc = acc + t[1] % 7 + t[60] % 5
end
print(acc)
"####,
    },
    Prog {
        name: "bp_floor",
        small: true,
        src: r####"
-- BUNDLE-P (`f2ifl`): `math.floor` / `math.ceil` of floats at the edges of the integer range, each value repeated in its own hot loop (so that a trace
-- specialises on it) and then all of them in one loop (every transition between the integer result, the float result and the guard exit).
local vals = { 0.5, -0.5, 1.5, -1.5, 3.0, -3.0, 0.0, -0.0, 2^53, -2^53, 2^62, -2^62, 2^63 - 1024, 2^63, -2^63, -2^63 - 1024 * 3,
  -9223372036854777856.0, 1e300, -1e300, 1 / 0, -1 / 0, 0 / 0, 123456.789, -123456.789, 4.5e15, 9007199254740993.0, 1e15 + 0.5 }
local out = {}
local function show(f) return math.type(f) .. ":" .. tostring(f) end
for k = 1, #vals do
  local v = vals[k]
  local a, c, n = 0, 0, 0
  local lf, lc
  for i = 1, 500 do
    local f = math.floor(v)
    local g = math.ceil(v)
    lf, lc = f, g
    if math.type(f) == "integer" then n = n + 1 end
    if math.type(g) == "integer" then c = c + 1 end
    a = a + (f == f and 1 or 0)
  end
  out[#out + 1] = string.format("%d %s %s %d %d %d", k, show(lf), show(lc), n, c, a)
end
print(table.concat(out, "\n"))
-- one loop over all values (floor results change type from iteration to iteration)
local acc, ints, flts, nans = 0, 0, 0, 0
for r = 1, 200 do
  for k = 1, #vals do
    local f = math.floor(vals[k])
    if math.type(f) == "integer" then ints = ints + 1; acc = acc ~ f else flts = flts + 1; if f ~= f then nans = nans + 1 end end
  end
end
print(acc, ints, flts, nans)
-- floor of an integer-valued float as a table key and as an array index (the `F2I` guards of the key conversions are not behind a floor)
local t = { 10, 20, 30, 40 }
local s = 0
local keys = { 1.0, 2.0, 2.5, 3.0, 4.0, 0.5, 1e100 }
for r = 1, 300 do
  for k = 1, #keys do
    local key = keys[k]
    s = s + (t[key] or 1) + (t[math.floor(key)] or 2)
  end
end
print(s)
"####,
    },
    Prog {
        name: "bp_floor_minint",
        small: true,
        src: r####"
-- BUNDLE-P (`f2ifl`): the floor of exactly -2^63 is the integer `math.mininteger` (the guard behind the floor exits there, conservatively; the result must not change).
local v = -2^63
local m = math.mininteger
local acc = 0
for i = 1, 600 do
  local f = math.floor(v)
  acc = acc + (f == m and 1 or 0) + (math.type(f) == "integer" and 1 or 0)
end
print(acc, math.floor(v), math.ceil(v), math.floor(-2^63 + 1024))
"####,
    },
    Prog {
        name: "bq_eq",
        small: true,
        src: r####"
-- BUNDLE-Q (t1q bit 0): == / ~= of objects in tier 1
local function eqf(a, b) return a == b end
local function nef(a, b) return a ~= b end
local out = {}
local cnt = 0
local function mm(r) return function() cnt = cnt + 1; return r end end
local plain1, plain2 = {}, {}
local mtn = {}; mtn.__index = mtn                       -- metatable without __eq
local o1, o2 = setmetatable({}, mtn), setmetatable({}, mtn)
local mt_t = { __eq = mm(true) }
local mt_f = { __eq = mm(false) }
local mt_one = { __eq = mm(1) }
local mt_nil = { __eq = mm(nil) }
local mt_str = { __eq = mm("x") }
local t1, t2 = setmetatable({}, mt_t), setmetatable({}, mt_t)
local f1, f2 = setmetatable({}, mt_f), setmetatable({}, mt_f)
local n1, n2 = setmetatable({}, mt_one), setmetatable({}, mt_nil)
local s1 = setmetatable({}, mt_str)
local mix_t = setmetatable({}, mt_t)                    -- __eq only on one side
local nomm = setmetatable({}, mtn)
local fn1, fn2 = function() end, function() end
local co1, co2 = coroutine.create(fn1), coroutine.create(fn1)
local big1, big2 = string.rep("a", 60) .. "x", string.rep("a", 60) .. "x"
local vals = { plain1, plain2, o1, o2, t1, t2, f1, f2, n1, n2, s1, mix_t, nomm, fn1, fn2, print, print, io.stdout, io.stderr, io.stdout,
  co1, co2, 1, 1.0, "a", "a", big1, big2, true, false, 0/0, 2^53, plain1, nomm }
local n = #vals
for rep = 1, 40 do
  for i = 1, n do
    for j = 1, n do
      local a, b = vals[i], vals[j]
      local r1, r2 = eqf(a, b), nef(a, b)
      if rep == 1 then out[#out + 1] = (r1 and "T" or "F") .. (r2 and "T" or "F") end
      cnt = cnt + (r1 and 1 or 0) + (r2 and 2 or 0)
    end
  end
end
print(table.concat(out, ","):sub(1, 400), #out, cnt)
-- the same table in different registers, in conditions
local acc = 0
for i = 1, 3000 do
  local a, b = vals[i % n + 1], vals[(i * 7) % n + 1]
  if a == b then acc = acc + 1 end
  if a ~= b then acc = acc + 3 end
  if a == a then acc = acc + 5 end
end
print(acc)
-- an __eq added/removed after the absence was cached (flags must be invalidated)
local mt = {}
local x, y = setmetatable({}, mt), setmetatable({}, mt)
local r = {}
for i = 1, 600 do
  if i == 100 then mt.__eq = function() return true end end
  if i == 200 then mt.__eq = nil end
  if i == 300 then rawset(mt, "__eq", function() return true end) end
  if i == 400 then mt.__eq = function() return false end end
  if i == 500 then mt.__eq = nil end
  r[#r + 1] = (x == y) and 1 or 0
end
print((table.concat(r):gsub("(.)%1*", "%1")), #r)
-- __eq only on the second operand's metatable
local A, B = setmetatable({}, {}), setmetatable({}, { __eq = function() return true end })
local c1, c2 = 0, 0
for i = 1, 600 do
  if A == B then c1 = c1 + 1 end
  if B == A then c2 = c2 + 1 end
  if i == 300 then getmetatable(A).__eq = function() return false end end
end
print(c1, c2)
-- setmetatable on one operand changes the answer
local P, Q = {}, {}
local seq = {}
for i = 1, 600 do
  if i == 200 then setmetatable(P, { __eq = function() return true end }) end
  if i == 400 then setmetatable(P, nil) end
  seq[#seq + 1] = (P == Q) and 1 or 0
end
print((table.concat(seq):gsub("(.)%1*", "%1")))
-- __eq erroring and yielding
local E1, E2 = setmetatable({}, { __eq = function() error("boom") end }), setmetatable({}, {})
local errs = 0
for i = 1, 600 do
  local ok = pcall(eqf, E1, E2)
  if not ok then errs = errs + 1 end
  local ok2, r2 = pcall(eqf, E1, E1)
  if ok2 and r2 then errs = errs + 1000 end
end
print(errs)
local co = coroutine.wrap(function()
  local Y1, Y2 = setmetatable({}, { __eq = function() coroutine.yield(1); return true end }), {}
  local k = 0
  for i = 1, 300 do if Y1 == Y2 then k = k + 1 end end
  return k
end)
local yy = 0
while true do local v = co(); if v ~= 1 then print("co", v) break end yy = yy + 1 end
print(yy)
"####,
    },
    Prog {
        name: "bq_mod",
        small: true,
        src: r####"
-- BUNDLE-Q (t1q bit 1): % with a float operand in tier 1
local vals = { 0, 1, -1, 2, -2, 5, -5, 7, 255, 256, 1000003, math.maxinteger, math.mininteger,
  0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 5.5, -5.5, 1e15, -1e15, 2^53, -2^53, 2^63, -2^63,
  1e308, -1e308, 1e-308, -1e-308, 5e-324, -5e-324, 1/0, -1/0, 0/0, 3.0, -3.0, 4.0, -4.0 }
local function z(x) if x == 0 then return (1 / x > 0) and "+0" or "-0" end return string.format("%.17g", x) end
local function fmt(ok, r)
  if not ok then return (tostring(r):gsub("^.-:%d+: ", "")) end
  return math.type(r) .. ":" .. z(r)
end
local h = 0
local function mix(s) for i = 1, #s do h = (h * 31 + s:byte(i)) % 2147483629 end end
local n = #vals
-- register form: all pairs
local f = function(a, b) return a % b end
for rep = 1, 3 do
  for i = 1, n do
    for j = 1, n do
      local ok, r = pcall(f, vals[i], vals[j])
      local s = fmt(ok, r)
      mix(s)
      if rep == 1 and (i + j) % 17 == 0 then print(z(vals[i]), z(vals[j]), s) end
    end
  end
end
print("pairs", h)
-- constant forms in a hot loop (so the loop body is compiled)
local acc = 0.0
local sg = 0
for rep = 1, 30 do
  for i = 1, n do
    local x = vals[i]
    local a1, a2, a3, a4, a5, a6, a7 = x % 2.5, x % -3.5, x % 1e308, x % 0.5, x % 256.0, x % -1e-300, x % (1/0)
    local ok, a8 = pcall(function() return x % 0 end)
    local a9 = x % 3.0
    local t = { a1, a2, a3, a4, a5, a6, a7, a9 }
    for k = 1, #t do
      local v = t[k]
      if v == v and v - v == 0 then acc = acc + v % 1e6 end
      sg = (sg * 7 + (v ~= v and 1 or (v == 0 and (1 / v > 0 and 2 or 3) or 4))) % 1000003
    end
    if rep == 1 then print(z(x), z(a1), z(a2), z(a3), z(a4), z(a5), z(a6), z(a7), fmt(ok, a8), z(a9)) end
  end
end
print(string.format("%.10g", acc), sg)
-- results stay floats and feed later arithmetic
local c = 0.0
for i = 1, 2000 do
  local fl = (i * 0.75) % 2
  c = c + fl + (i % 2.0) + ((-i) % 3.5)
end
print(string.format("%.6f", c))
-- integer fast path unchanged: zero divisor error, minint % -1
local e = 0
for i = 1, 500 do
  local d = (i % 7 == 0) and 0 or 3
  local ok = pcall(function() return i % d end)
  if not ok then e = e + 1 end
  e = e + (math.mininteger % (-1 - (i % 2 - 1) * 0)) + (i % -4)
end
print(e)
"####,
    },
    Prog {
        name: "bp_rearm_young_la",
        small: true,
        src: r####"
-- BUNDLE-P / C45 (`rearmside`, `rearmclean`): the clean twin whose class `A` method has a `while` loop (a callee loop unrolled into the caller's trace under `loopany`:
-- `Trace::la`), so the family is not clean even where a side recording has not failed yet.
local A, B, C, D = {}, {}, {}, {}
for _, K in ipairs { A, B, C, D } do K.__index = K end
function A:step(d) local j = 0; while j < 3 do self.x = self.x + d; j = j + 1 end; return self.x end
function B:step(d) self.x = self.x - d * 0.5; return self.x end
function C:step(d) if self.x > 100 then self.x = 0 end; self.x = self.x + d * 2; return self.x end
function D:step(d) self.y = self.y + 1; self.x = self.x + self.y % 3; return self.x end
local objs = {}
for i = 1, 40 do
  local K = ({ A, B, C, D })[i % 4 + 1]
  objs[i] = setmetatable({ x = i, y = 0 }, K)
end
local acc = 0
local function pass()
  for i = 1, #objs do
    acc = acc + objs[i]:step(1)
  end
end
local function run(k)
  if k == 0 then return end
  pass()
  return run(k - 1)
end
for step = 1, 220 do pass() end
print(string.format("%.3f", acc))
"####,
    },
    Prog {
        name: "bp_rearm_young_clean",
        small: true,
        src: r####"
-- BUNDLE-P / C45 (`rearmside`, `rearmclean`), the CLEAN twin: a hot loop in a function (called 220 times from a main-chunk loop) whose callee dispatches over four classes
-- (one side trace per class); leaving the loop continues in compiled code, so no side recording fails.
local A, B, C, D = {}, {}, {}, {}
for _, K in ipairs { A, B, C, D } do K.__index = K end
function A:step(d) self.x = self.x + d; return self.x end
function B:step(d) self.x = self.x - d * 0.5; return self.x end
function C:step(d) if self.x > 100 then self.x = 0 end; self.x = self.x + d * 2; return self.x end
function D:step(d) self.y = self.y + 1; self.x = self.x + self.y % 3; return self.x end
local objs = {}
for i = 1, 40 do
  local K = ({ A, B, C, D })[i % 4 + 1]
  objs[i] = setmetatable({ x = i, y = 0 }, K)
end
local acc = 0
local function pass()
  for i = 1, #objs do
    acc = acc + objs[i]:step(1)
  end
end
local function run(k)
  if k == 0 then return end
  pass()
  return run(k - 1)
end
for step = 1, 220 do pass() end
print(string.format("%.3f", acc))
"####,
    },
    Prog {
        name: "bp_rearm_young_dirty",
        small: true,
        src: r####"
-- BUNDLE-P / C45 (`rearmside`, `rearmclean`), the DIRTY twin (the loop exit is a failed side recording `SideUntraced`): a hot loop whose callee dispatches over several classes (each exit needs its own side trace) and that is left through exits many
-- times before its root trace has the 1024 entries the R1 rule asks for.
local A, B, C, D = {}, {}, {}, {}
for _, K in ipairs { A, B, C, D } do K.__index = K end
function A:step(d) self.x = self.x + d; return self.x end
function B:step(d) self.x = self.x - d * 0.5; return self.x end
function C:step(d) if self.x > 100 then self.x = 0 end; self.x = self.x + d * 2; return self.x end
function D:step(d) self.y = (self.y or 0) + 1; self.x = self.x + self.y % 3; return self.x end
local objs = {}
for i = 1, 40 do
  local K = ({ A, B, C, D })[i % 4 + 1]
  objs[i] = setmetatable({ x = i }, K)
end
local acc = 0
for step = 1, 220 do
  for i = 1, #objs do
    acc = acc + objs[i]:step(1)
  end
  if step % 50 == 0 then objs[step % 40 + 1] = setmetatable({ x = step }, ({ B, C, D, A })[step % 4 + 1]) end
end
print(string.format("%.3f", acc))
"####,
    },
    Prog {
        name: "bs_amt",
        small: true,
        src: r####"
-- BUNDLE-S S2 (amt): array reads and stores of tables that have a metatable
local log = {}
local function note(...) local p = {} for i = 1, select("#", ...) do p[i] = tostring((select(i, ...))) end log[#log + 1] = table.concat(p, ":") end
-- reads: no __index at all, a Lua __index, a table __index; elements present and nil
local plainmt = setmetatable({}, { __metatable = nil })
local function mk(kind, n)
  local t = {}
  for i = 1, n do t[i] = i * 2 end
  if kind == "none" then return setmetatable(t, { __tostring = function() return "x" end }) end
  if kind == "fn" then return setmetatable(t, { __index = function(_, k) return "fn" .. tostring(k) end }) end
  if kind == "tab" then return setmetatable(t, { __index = { [3] = "T3", [9] = "T9" } }) end
  return t
end
local function sum(t, n)
  local s = 0
  for i = 1, n do
    local v = t[i]
    if type(v) == "number" then s = s + v else s = s + #tostring(v) * 1000 end
  end
  return s
end
for _, kind in ipairs { "plain", "none", "fn", "tab" } do
  local t = mk(kind, 50)
  local r = 0
  for rep = 1, 40 do r = r + sum(t, 50) end
  note(kind, r, sum(t, 60))        -- 51..60 are absent: __index answers for "fn" / "tab"
end
-- a hole punched in the middle of the loop: __index starts firing
do
  local t = mk("fn", 50)
  local res = {}
  for rep = 1, 120 do
    if rep == 60 then rawset(t, 7, nil) end
    if rep == 90 then t[7] = 14 end
    res[#res + 1] = sum(t, 10)
  end
  note("hole", res[1], res[59], res[60], res[89], res[90], res[120])
end
-- the metatable gains __index after the absence of it was cached; setmetatable(t, nil); a different metatable
do
  local mt = { __tostring = tostring }
  local t = setmetatable({}, mt)
  for i = 1, 30 do t[i] = i end
  t[15] = nil
  local res = {}
  for rep = 1, 200 do
    if rep == 50 then mt.__index = function(_, k) return -k end end
    if rep == 100 then mt.__index = nil end
    if rep == 130 then setmetatable(t, nil) end
    if rep == 160 then setmetatable(t, { __index = function(_, k) return 1000 + k end }) end
    local s = 0
    for i = 10, 20 do s = s + (t[i] or 0) end
    res[#res + 1] = s
  end
  note("mtchange", res[1], res[49], res[50], res[99], res[100], res[129], res[130], res[159], res[160], res[200])
end
-- one loop over tables with and without metatable
do
  local ts = {}
  for i = 1, 8 do
    ts[i] = mk(i % 3 == 0 and "fn" or (i % 3 == 1 and "plain" or "none"), 20)
  end
  local s = 0
  for rep = 1, 100 do
    for j = 1, #ts do
      local t = ts[j]
      for i = 15, 25 do
        local v = t[i]
        s = s + (type(v) == "number" and v or (v and 7 or 1))
      end
    end
  end
  note("mixed", s)
end
-- keys: zero, negative, huge, float with integer value, float, string "1"
do
  local t = setmetatable({ 10, 20, 30 }, { __index = function(_, k) return "m" .. tostring(k) end })
  local keys = { 1, 2, 3, 0, -1, 4, 2^53, 2.0, 3.0, 1.5, "1", math.maxinteger, math.mininteger }
  local out = {}
  for rep = 1, 100 do
    for i = 1, #keys do out[i] = t[keys[i]] end
  end
  note("keys", table.concat(out, ","))
end
-- stores: existing slots are raw even with __newindex; nil slots call it; __newindex added later; rawset
do
  local seen = {}
  local mt = {}
  local t = setmetatable({}, mt)
  for i = 1, 20 do t[i] = i end
  local function fill(n, v)
    for i = 1, n do t[i] = v + i end
  end
  for rep = 1, 100 do
    if rep == 40 then mt.__newindex = function(tt, k, v) seen[#seen + 1] = k; rawset(tt, k, v * 2) end end
    if rep == 50 then t[5] = nil end
    if rep == 70 then mt.__newindex = nil end
    if rep == 80 then t[6] = nil end
    if rep == 90 then rawset(mt, "__newindex", function(tt, k, v) seen[#seen + 1] = -k end) end
    if rep == 95 then t[7] = nil end
    fill(10, rep)
  end
  local parts = {}
  for i = 1, 25 do parts[#parts + 1] = tostring(t[i]) end
  note("stores", table.concat(parts, ","), #seen, table.concat(seen, ","))
end
-- stores of changing types over the existing elements of a table whose metatable has a __newindex (the old value is guarded by its type)
do
  local cnt = 0
  local t = setmetatable({}, { __newindex = function(tt, k, v) cnt = cnt + 1; rawset(tt, k, v) end })
  for i = 1, 16 do t[i] = i end
  local vals = { 1, 2.5, "s", true, {}, 3, false }
  for rep = 1, 200 do
    for i = 1, 16 do t[i] = vals[(i + rep) % 7 + 1] end
    if rep == 100 then t[3] = nil; t[9] = nil end
  end
  local parts = {}
  for i = 1, 16 do parts[#parts + 1] = type(t[i]) end
  note("mixedstores", cnt, table.concat(parts, ","))
end
-- stores over a nil slot of a table with a read-only __newindex that errors
do
  local t = setmetatable({ 1, 2, 3, 4 }, { __newindex = function() error("ro") end })
  local errs = 0
  for rep = 1, 150 do
    if rep == 100 then t[2] = nil end
    local ok = pcall(function() for i = 1, 4 do t[i] = rep end end)
    if not ok then errs = errs + 1 end
  end
  note("ro", errs, t[1], t[2], t[3], t[4])
end
-- a __newindex that itself writes the array (the slot is still nil afterwards or filled)
do
  local store = {}
  local t = setmetatable({ 1, 2, 3 }, { __newindex = function(tt, k, v) store[k] = v end })
  t[2] = nil
  for rep = 1, 100 do
    for i = 1, 3 do t[i] = rep + i end
  end
  note("redir", t[1], t[2], t[3], store[2], store[1])
end
-- negative / zero / float keys on stores
do
  local t = setmetatable({ 1, 2, 3 }, { __newindex = function(tt, k, v) rawset(tt, k, "n" .. tostring(v)) end })
  local ks = { 1, 3, 0, -2, 2.0, 1.5, 4 }
  for rep = 1, 60 do
    for i = 1, #ks do t[ks[i]] = rep end
  end
  local parts = {}
  for i = -2, 5 do parts[#parts + 1] = tostring(t[i]) end
  note("fkeys", table.concat(parts, ","), t[1.5])
end
-- inside a coroutine; the __index yields
do
  local t = setmetatable({}, { __index = function(_, k) return coroutine.yield(k) end })
  for i = 1, 10 do t[i] = i end
  t[4] = nil
  local co = coroutine.create(function()
    local s = 0
    for rep = 1, 60 do
      for i = 1, 10 do s = s + t[i] end
    end
    return s
  end)
  local _, r = coroutine.resume(co)
  local n = 0
  while coroutine.status(co) == "suspended" do n = n + 1; _, r = coroutine.resume(co, n) end
  note("yield", r, n)
end
for i = 1, #log do print(log[i]) end
"####,
    },
    Prog {
        name: "bs_ixmm",
        small: true,
        src: r####"
-- BUNDLE-S S1 (t1q bits 2 / 3): Lua __index / __newindex functions run as in-place frames of tier 1
local log = {}
local function note(...)
  local p = {}
  for i = 1, select("#", ...) do p[i] = tostring((select(i, ...))) end
  log[#log + 1] = table.concat(p, ":")
end
-- 1. getfield / gettable (string and integer keys) / geti with a function __index
do
  local calls = 0
  local t = setmetatable({ a = 1, 10, 20 }, { __index = function(tt, k) calls = calls + 1; return type(k) == "number" and k * 3 or k .. "?" end })
  local function f(n)
    local s = 0
    for i = 1, n do
      s = s + t.a + t[1] + t[i % 7 + 3] + #t.zz + t[5] + #t["q" .. (i % 3)]
    end
    return s
  end
  note("get", f(300), f(5), calls)
end
-- 2. SELF through a class whose __index is a function; the receiver register is also the destination
do
  local cls = setmetatable({}, { __index = function(_, name) return function(self, x) return name .. x .. (self.v or "-") end end })
  local o = setmetatable({ v = "V" }, { __index = cls })
  local function f(n)
    local acc = 0
    for i = 1, n do
      acc = acc + #o:foo(i) + #o:bar("z")
    end
    return acc
  end
  note("self", f(300), o:baz(1))
end
-- 3. chains: table -> table -> function; the function gets the table that holds the metatable
do
  local seen = {}
  local deep = setmetatable({}, { __index = function(tt, k) seen[#seen + 1] = tt; return k end })
  local mid = setmetatable({}, { __index = deep })
  local top = setmetatable({}, { __index = mid })
  local function f(n)
    local s = 0
    for i = 1, n do s = s + #top.abc + (top[i] or 0) end
    return s
  end
  note("chain", f(200), seen[1] == deep, seen[2] == deep, #seen)
end
-- 4. a global environment with an __index function (GETTABUP)
do
  local env = setmetatable({}, { __index = function(_, k) return k end })
  local chunk = load("local n = ... local s = 0 for i = 1, n do s = s + #alpha + #beta end return s", "c", "t", env)
  note("env", chunk(300), chunk(2))
  local seen = 0
  local env2 = setmetatable({ print = print }, { __index = function(_, k) seen = seen + 1; return nil end })
  local c2 = load("local n = ... local c = 0 for i = 1, n do if missing == nil then c = c + 1 end end return c", "c2", "t", env2)
  note("env2", c2(200), seen)
end
-- 5. handlers returning none / many values; tail calls; varargs
do
  local function none() end
  local function many() return 1, 2, 3 end
  local t1 = setmetatable({}, { __index = function() end })
  local t2 = setmetatable({}, { __index = function() return 1, 2, 3 end })
  local t3 = setmetatable({}, { __index = function(t, k) return many() end })
  local t4 = setmetatable({}, { __index = function(...) return select("#", ...) end })
  local t5 = setmetatable({}, { __index = function(t, k) return (many()) end })
  local function f(n)
    local s = 0
    for i = 1, n do
      s = s + (t1.x == nil and 1 or 0) + t2.x + t3.y + t4.z + t5.w
    end
    return s
  end
  note("rets", f(300))
end
-- 6. errors in the handler, caught by pcall; error with a traceback from the handler; erroring after a few good lookups
do
  local n = 0
  local t = setmetatable({}, { __index = function(_, k) n = n + 1; if n % 50 == 0 then error("boom" .. n, 0) end; return n end })
  local function f(m)
    local s, errs = 0, 0
    for i = 1, m do
      local ok, v = pcall(function() return t.x end)
      if ok then s = s + v else errs = errs + 1 end
    end
    return s, errs
  end
  note("err", f(200))
  local t2 = setmetatable({}, { __index = function(_, k) error({ code = k }) end })
  local ok, e = pcall(function() return t2.zzz end)
  note("errobj", ok, type(e), e.code)
  local t3 = setmetatable({}, { __index = function(_, k) return debug.traceback("tb", 1) end })
  local function g() return t3.q end
  local tb
  for i = 1, 100 do tb = g() end
  note("tb", (tb:gsub("0x%x+", "PTR"):gsub("\n%s+%[C%]:[^\n]*", "")))
end
-- 7. debug functions inside the handler
do
  local info
  local t = setmetatable({}, { __index = function(tt, k)
    local a = 5
    local i = debug.getinfo(1, "nSl")
    local j = debug.getinfo(2, "nl")
    info = tostring(i.name) .. "/" .. tostring(i.namewhat) .. "/" .. tostring(j.currentline > 0)
    local n, v = debug.getlocal(1, 3)
    debug.setlocal(1, 3, 6)
    return a + (v or 0) + #k
  end })
  local function f(m)
    local s = 0
    for i = 1, m do s = s + t.abc end
    return s
  end
  note("dbg", f(100), info)
  -- a hook installed from the handler
  local cnt = 0
  local th = setmetatable({}, { __index = function(_, k)
    if k == "hook" then debug.sethook(function() cnt = cnt + 1 end, "", 100) end
    if k == "unhook" then debug.sethook() end
    return 1
  end })
  local function g()
    local s = 0
    for i = 1, 400 do
      s = s + th.x
      if i == 100 then s = s + th.hook end
      if i == 300 then s = s + th.unhook end
    end
    return s
  end
  note("hook", g(), cnt > 0)
end
-- 8. mutation during the handler / the loop
do
  local calls = { 0, 0 }
  local mt = {}
  local t = setmetatable({}, mt)
  mt.__index = function(_, k) calls[1] = calls[1] + 1; return 1 end
  local res = {}
  local function f(m)
    local s = 0
    for i = 1, m do
      if i == 100 then mt.__index = function(_, k) calls[2] = calls[2] + 1; return 2 end end
      if i == 200 then rawset(t, "x", 5) end
      if i == 250 then t.x = nil end
      if i == 300 then setmetatable(t, nil) end
      if i == 350 then setmetatable(t, { __index = function() return 7 end }) end
      if i == 380 then mt.__index = nil end
      s = s + (t.x or 0)
    end
    return s
  end
  note("mut", f(400), calls[1], calls[2])
  -- the handler changes its own metatable
  local mt2 = {}
  local u = setmetatable({}, mt2)
  local k = 0
  mt2.__index = function(tt, key)
    k = k + 1
    if k == 10 then mt2.__index = function() return -1 end end
    if k == 20 then rawset(tt, key, 99) end
    return k
  end
  local s = 0
  for i = 1, 100 do s = s + u.y end
  note("mut2", s, k)
end
-- 9. the handler yields (inside a coroutine)
do
  local t = setmetatable({}, { __index = function(_, k) return coroutine.yield(k) end })
  local co = coroutine.create(function()
    local s = 0
    for i = 1, 100 do s = s + t.a + t[i] end
    return s
  end)
  local ok, v = coroutine.resume(co)
  local n = 0
  while coroutine.status(co) == "suspended" do n = n + 1; ok, v = coroutine.resume(co, n % 7) end
  note("yield", ok, v, n)
  -- a metamethod called from a coroutine that finishes inside the handler
  local co2 = coroutine.wrap(function()
    local u = setmetatable({}, { __index = function(_, k) if k == "stop" then coroutine.yield("stopping") end return 1 end })
    local s = 0
    for i = 1, 50 do s = s + u.go end
    s = s + u.stop
    for i = 1, 50 do s = s + u.go end
    return s
  end)
  note("yield2", co2(), co2())
end
-- 10. recursion: a handler that indexes another table with a handler (nested frames); runaway recursion
do
  local depth = 0
  local a, b
  a = setmetatable({}, { __index = function(_, k) return b[k] + 1 end })
  b = setmetatable({}, { __index = function(_, k) return k end })
  local s = 0
  for i = 1, 200 do s = s + a[i] end
  note("nest", s)
  local rec = setmetatable({}, { __index = function(t, k) return t[k] end })
  local ok, e = pcall(function() return rec.x end)
  note("runaway", ok, (tostring(e):gsub("^.-:%d+: ", "")))
  local cnt = 0
  local rec2 = setmetatable({}, { __index = function(t, k) cnt = cnt + 1; if cnt < 150 then return t[k + 1] end return k end })
  note("deep", rec2[1])
end
-- 11. not affected: strings, numbers, userdata, tables without metatable
do
  local s = "hello"
  local acc = 0
  for i = 1, 200 do acc = acc + #s:upper() + s:len() + (s.len and 1 or 0) end
  note("str", acc)
  local ok, e = pcall(function() local n = 5; return n.x end)
  note("num", ok, (tostring(e):gsub("^.-:%d+: ", "")))
  local ok2, e2 = pcall(function() local n; return n.x end)
  note("nil", ok2, (tostring(e2):gsub("^.-:%d+: ", "")))
  local f = io.stdout
  local c = 0
  for i = 1, 100 do if f.write then c = c + 1 end end
  note("ud", c)
  local pt = {}
  local m = 0
  for i = 1, 200 do m = m + (pt.zz == nil and 1 or 0) end
  note("plain", m)
end
-- 12. array keys vs hash keys, negative / zero / huge / float keys, holes
do
  local t = setmetatable({ 1, 2, nil, 4 }, { __index = function(_, k) return "idx" .. tostring(k) end })
  local keys = { 1, 2, 3, 4, 5, 0, -1, 2^53, 1.5, 2.0, "x", math.maxinteger, math.mininteger, true }
  local out = {}
  local function f()
    for i = 1, #keys do out[i] = tostring(t[keys[i]]) end
  end
  for rep = 1, 100 do f() end
  note("keys", table.concat(out, ","))
end
-- 13. __newindex functions
do
  local shadow = {}
  local mt = { __newindex = function(t, k, v) shadow[k] = (shadow[k] or 0) + v end }
  local t = setmetatable({ a = 1 }, mt)
  local function f(n)
    for i = 1, n do
      t.a = i                -- existing key: raw
      t.b = i                -- absent: __newindex
      t[i % 5 + 1] = i       -- absent integer keys
      t["k" .. i % 3] = i
    end
  end
  f(300)
  note("newindex", t.a, shadow.b, shadow[1], shadow[3], shadow.k1, rawget(t, "b"))
  -- the metamethod becomes a rawset, then goes away, then comes back
  local log2 = {}
  local mt2 = {}
  local u = setmetatable({}, mt2)
  mt2.__newindex = function(tt, k, v) log2[#log2 + 1] = k; rawset(tt, k, v) end
  local function g(n)
    for i = 1, n do
      if i == 100 then mt2.__newindex = nil end
      if i == 150 then mt2.__newindex = function(tt, k, v) log2[#log2 + 1] = -k end; u[1] = nil end
      u[i % 50] = i
    end
  end
  g(200)
  note("nirawset", #log2, log2[1], log2[#log2], u[7])
  -- errors / yields in __newindex
  local ro = setmetatable({}, { __newindex = function(_, k) error("ro " .. tostring(k), 0) end })
  local errs = 0
  for i = 1, 100 do if not pcall(function() ro[i] = 1 end) then errs = errs + 1 end end
  local t3 = setmetatable({}, { __newindex = function(tt, k, v) coroutine.yield(k); rawset(tt, k, v) end })
  local co = coroutine.wrap(function() for i = 1, 60 do t3[i] = i end return "done" end)
  local last
  repeat last = co() until last == "done"
  note("nierr", errs, #t3, t3[60])
  -- __newindex as a table, nested; SETTABUP through an environment
  local sink = {}
  local t4 = setmetatable({}, { __newindex = setmetatable({}, { __newindex = function(_, k, v) sink[k] = v end }) })
  for i = 1, 100 do t4.q = i end
  local e = setmetatable({}, { __newindex = function(tt, k, v) sink["g" .. k] = v end })
  local chunk = load("for i = 1, 100 do gv = i end", "c", "t", e)
  chunk()
  note("nitab", sink.q, sink.gvv)
  -- the handler for a SETI with a float key and a negative key
  local keys = {}
  local t5 = setmetatable({}, { __newindex = function(tt, k, v) keys[#keys + 1] = tostring(k) .. "=" .. tostring(v) end })
  for i = 1, 20 do t5[-1] = i; t5[0] = i; t5[1.5] = i; t5[2^53] = i; t5[i] = i end
  note("nikeys", #keys, keys[1], keys[2], keys[3], keys[4], keys[5])
end
-- 14. handlers that return closures capturing their own locals (the general RETURN with open upvalues to close), upvalues shared by closures, to-be-closed variables,
-- vararg handlers
do
  local made = {}
  local t = setmetatable({}, { __index = function(_, k)
    local cnt = 0
    local name = "f" .. tostring(k)
    local f = function() cnt = cnt + 1; return name .. cnt end
    made[#made + 1] = f
    if k % 3 == 0 then return f end
    return function() return cnt, name end
  end })
  local function drive(n)
    local s = 0
    for i = 1, n do
      local g = t[i % 5]
      local a, b = g()
      s = s + (type(a) == "number" and a or #a) + #tostring(b or "")
    end
    return s
  end
  note("closure", drive(300), #made, made[1](), made[1](), made[4]())
  local order = {}
  local tb = setmetatable({}, { __index = function(_, k)
    local x <close> = setmetatable({}, { __close = function() order[#order + 1] = "close" .. k end })
    return function() return x end
  end })
  local ok = 0
  for i = 1, 100 do local f = tb[i % 4]; if type(f) == "function" then ok = ok + 1 end end
  note("tbc", ok, #order, order[1], order[#order])
  local va = setmetatable({}, { __index = function(...)
    local n = select("#", ...)
    return function() return n end
  end })
  local sv = 0
  for i = 1, 100 do sv = sv + va[i]() end
  note("vararg", sv)
  -- upvalues closed by the handler's return must keep the value of that call, per call
  local keep = setmetatable({}, { __index = function(_, k) local v = k * 2; return function() v = v + 1; return v end end })
  local fs = {}
  for i = 1, 50 do fs[i] = keep[i] end
  local tot = 0
  for r = 1, 3 do for i = 1, 50 do tot = tot + fs[i]() end end
  note("per-call", tot)
  -- the same for __newindex handlers that return values / closures
  local store = {}
  local tn = setmetatable({}, { __newindex = function(_, k, v) local w = v; store[k] = function() return w end; return function() return w end, 1 end })
  for i = 1, 100 do tn[i] = i * 3 end
  local ss = 0
  for i = 1, 100 do ss = ss + store[i]() end
  note("newindex-closure", ss)
end
-- 15. a handler that changes a local of the function that triggered it (debug.setlocal): the code after the access must not trust the static type of that register
do
  local flip = 0
  local t = setmetatable({}, { __index = function(_, k)
    flip = flip + 1
    if flip % 50 == 0 then debug.setlocal(2, 2, 2.5) end
    if flip % 70 == 0 then debug.setlocal(2, 2, "7") end
    if flip % 90 == 0 then debug.setlocal(2, 2, 3) end
    return k
  end })
  local function f(n)
    local s = 0
    for i = 1, n do
      local v = t.x
      s = s + 1
      s = s * 1
    end
    return s
  end
  local function g(n)
    local s = 0
    for i = 1, n do
      s = s + #t[i]
    end
    return s
  end
  local ok1, r1 = pcall(f, 400)
  local ok2, r2 = pcall(g, 400)
  note("setlocal", ok1, r1, ok2, r2)
  local f2 = 0
  local u = setmetatable({}, { __newindex = function(tt, k, v)
    f2 = f2 + 1
    if f2 % 40 == 0 then debug.setlocal(2, 2, 0.25) end
    rawset(tt, k, v)
  end })
  local function h(n)
    local s = 0
    for i = 1, n do
      u[i] = i
      s = s + 1
    end
    return s
  end
  note("setlocal2", pcall(h, 300))
  -- straight-line code: the register is typed by the LOADI of the same block
  local cnt3 = 0
  local t3 = setmetatable({}, { __index = function(_, k)
    cnt3 = cnt3 + 1
    if cnt3 % 7 == 0 then debug.setlocal(2, 1, 2.5) end
    if cnt3 % 11 == 0 then debug.setlocal(2, 1, "4") end
    return k
  end })
  local function k3()
    local s = 0
    local v = t3.x
    s = s + 1
    local w = t3.y
    s = s * 2
    return s
  end
  local acc3 = {}
  for i = 1, 300 do
    local ok, r = pcall(k3)
    acc3[#acc3 + 1] = ok and tostring(r) or "E"
  end
  note("setlocal3", table.concat(acc3, ",", 1, 30), #acc3)
end
for i = 1, #log do print(log[i]) end
"####,
    },
    Prog {
        name: "br_lib",
        small: true,
        src: r####"
-- BUNDLE-R `libcall` bits 5.. : table.unpack, table.move, tonumber of strings, in loops that change shape
local out = {}
local function P(...) local t = table.pack(...) for i = 1, t.n do t[i] = (tostring(t[i]):gsub("0x%x+", "ADDR")) end out[#out + 1] = table.concat(t, ",") end
local unpack, move = table.unpack, table.move
-- unpack: fixed sizes, the size changing, nil holes, a metatable, wanted counts fewer / more than the size
local function u1(n) local acc = 0 for i = 1, n do local t = { 1, 2, i, 4, 5 } local a, b, c, d, e = unpack(t) acc = acc + e + c + b end return acc end
P(u1(900))
local function u2(n) local acc, cnt = 0, 0 for i = 1, n do local t = {} for j = 1, i % 7 do t[j] = j * i end local a, b, c = unpack(t) acc = acc + (a or 0) + (b or 0) * 3 + (c or 0) * 5 cnt = cnt + select("#", unpack(t)) end return acc, cnt end
P(u2(900))
local function u3(n) local acc = 0 for i = 1, n do local t = { i, i + 1, i + 2 } local a, b, c, d, e, f = unpack(t) acc = acc + a + c + (d or 100) + (f or 7) end return acc end
P(u3(700))
local function u4(n) local acc = 0 for i = 1, n do local t = { i, i * 2 } local a = unpack(t) acc = acc + a end return acc end
P(u4(700))
local function u5(n) local acc = 0 local f = function(...) return select("#", ...) + (select(2, ...) or 0) end for i = 1, n do local t = { i, i + 1, i + 2, i + 3 } acc = acc + f(unpack(t)) + #{ unpack(t) } end return acc end
P(u5(700))
local function u6(n) local acc = 0 local function g(t) return unpack(t) end for i = 1, n do local a, b = g({ i, 3 * i }) acc = acc + a + b end return acc end
P(u6(700))
-- holes: a border is not unique
local function u7(n) local acc = 0 for i = 1, n do local t = { 1, nil, 3 } if i % 3 == 0 then t[2] = 2 end local a, b, c = unpack(t) acc = acc + (a or 0) + (b or 0) + (c or 0) end return acc end
P(u7(600))
local function u8(n) local acc = 0 for i = 1, n do local t = { 1, 2, 3, 4 } t[4] = nil if i % 5 == 0 then t[3] = nil end local a, b, c, d = unpack(t) acc = acc + (a or 0) + (b or 0) + (c or 0) + (d or 0) end return acc end
P(u8(600))
-- more elements than the recorder takes, and the size changing in the middle of the loop
local big = {} for i = 1, 40 do big[i] = i end
local function u9(n) local acc = 0 for i = 1, n do acc = acc + select("#", unpack(big)) + (unpack(big)) end return acc end
P(u9(500))
local function u10(n) local acc = 0 local t = { 1, 2, 3 } for i = 1, n do if i == n // 2 then t[4] = 4 t[5] = 5 end local a, b, c, d, e = unpack(t) acc = acc + a + b + c + (d or 0) + (e or 0) end return acc end
P(u10(800))
-- a hash part; a table with the border in the hash
local function u11(n) local acc = 0 for i = 1, n do local t = { 1, 2, 3, x = i } local a, b, c = unpack(t) acc = acc + a + b + c + t.x end return acc end
P(u11(600))
local function u12(n) local acc = 0 for i = 1, n do local t = {} t[1] = 1 t[2] = 2 t[3] = 3 t[4] = 4 local a, b, c, d = unpack(t) acc = acc + a + b + c + d end return acc end
P(u12(600))
-- metatables: __index only answers nil elements, __len changes the count
local mt = { __index = function(t, k) return k * 100 end }
local function u13(n) local acc = 0 for i = 1, n do local t = setmetatable({ 1, 2, 3 }, mt) local a, b, c, d = unpack(t) acc = acc + a + b + c + (d or 0) end return acc end
P(u13(500))
local mt2 = { __len = function() return 2 end }
local function u14(n) local acc = 0 for i = 1, n do local t = setmetatable({ 1, 2, 3 }, mt2) local a, b, c = unpack(t) acc = acc + a + b + (c or -1) end return acc end
P(u14(500))
local function u15(n) local acc = 0 local t = { 1, 2, 3 } for i = 1, n do if i == n // 2 then setmetatable(t, { __len = function() return 1 end }) end local a, b = unpack(t) acc = acc + a + (b or 0) end return acc end
P(u15(600))
-- other arguments and errors
P(pcall(unpack)) P(pcall(unpack, nil)) P(pcall(unpack, "abc")) P(pcall(unpack, 1)) P(pcall(unpack, {}, 1, 1e7)) P(pcall(unpack, {}, 1, math.maxinteger)) P(unpack({ 1, 2, 3 }, 2)) P(unpack({ 1, 2, 3 }, 2, 3)) P(unpack({ 1, 2, 3 }, -1, 1)) P(unpack({}, 1, 3))
local function u16(n) local acc = 0 for i = 1, n do local t = i == n - 3 and "str" or { i, 2 } local ok, a, b = pcall(unpack, t) acc = acc + (ok and a or 0) end return acc end
P(u16(500))
local function u17(n) local acc = 0 for i = 1, n do local t = i == n - 3 and 5 or { i, 2 } acc = acc + (unpack(t)) end return acc end
P(pcall(u17, 500))
P(select("#", unpack({ n = 3 })), select("#", unpack({ 1, 2, nil, 4 })), #{ unpack({ 1, 2, 3 }) })
-- floats and strings as elements, tables as elements (write barrier of the stores that follow)
local function u18(n) local acc = {} for i = 1, n do local t = { i + 0.5, "s" .. i, { i } } local a, b, c = unpack(t) acc[#acc + 1] = a + #b + c[1] end return #acc, acc[#acc], acc[1] end
P(u18(600))
-- tonumber of strings that are not numerals (nil), numerals, and the mixture: the success / failure flips in the middle of the loop
local forms = { "abc", "", "  ", "0x", "1e", "1e+", "5 5", "1_0", "inf", "nan", "0x1p", "--1", "+-2", "12abc", "abc12", "1,5", ".", "e5", "0b11", "1 2", "\0", "1\0", "x" .. ("y"):rep(60), ("9"):rep(30) .. "z" }
local good = { "12", "  34  ", "0x10", "-7", "1e2", ".5", "5.", "0x.8", "0x1p4", "  -0x7  ", "1e-2", ("9"):rep(30), "9223372036854775807", "9223372036854775808", "-9223372036854775808" }
local function tn1(n) local acc = 0 for i = 1, n do local v = tonumber(forms[i % #forms + 1]) if not v then acc = acc + 1 end end return acc end
P(tn1(1500))
local function tn2(n) local acc, fl = 0, 0 for i = 1, n do local s = (i % 3 == 0) and good[i % #good + 1] or forms[i % #forms + 1] local v = tonumber(s) if v then acc = acc + 1 if math.type(v) == "float" then fl = fl + 1 end end end return acc, fl end
P(tn2(1800))
local function tn3(n) local acc = 0 for i = 1, n do local s = i < n // 2 and "zzz" or "123" local v = tonumber(s) acc = acc + (v or -1) end return acc end
P(tn3(900))
local function tn4(n) local acc = 0 for i = 1, n do local s = i < n // 2 and "123" or "zzz" local v = tonumber(s) acc = acc + (v or -1) end return acc end
P(tn4(900))
local function tn5(n) local t = {} for i = 1, n do t[#t + 1] = tostring(tonumber("k" .. i)) .. "|" .. tostring(tonumber(" " .. i .. " ")) end return t[1], t[n], #t end
P(tn5(500))
local function tn6(n) local acc = 0 for i = 1, n do local v = tonumber(forms[i % 6 + 1]) or tonumber(good[i % 6 + 1]) acc = acc + v end return acc end
P(tn6(600))
P(pcall(tonumber)) P(tonumber(nil)) P(tonumber({})) P(tonumber(true)) P(tonumber("10", 2), tonumber("zz", 36), tonumber("8", 8), tonumber("7", 8)) P(pcall(tonumber, "10", 1)) P(pcall(tonumber, "10", 37)) P(pcall(tonumber, 10, 10))
P(tonumber("1e1", 16), tonumber(" ff ", 16), tonumber("-ff", 16), tonumber("", 10), tonumber("1.5", 10))
local function tn7(n) local acc = 0 for i = 1, n do acc = acc + (tonumber(forms[i % #forms + 1], 36) or 0) + (tonumber("11", 2) or 0) end return acc end
P(tn7(600))
local function tn8(n) local acc = 0 for i = 1, n do local s = i == n - 5 and {} or "x" .. i acc = acc + ((tonumber(s)) and 1 or 0) end return acc end
P(tn8(500))
local function tn9(n) local acc = 0 for i = 1, n do local s = i % 10 == 0 and i or "q" .. i acc = acc + ((tonumber(s)) and 1 or 0) end return acc end
P(tn9(900))
local function tn10(n) local acc = 0 for i = 1, n do local s = i % 7 == 0 and ("%d.0"):format(i) or "w" .. i local v = tonumber(s) if v then acc = acc + v end end return acc end
P(tn10(900))
local mixed = { "abc", "12", "x", "7.5", "", "0x10", "zz", "1e1" }
local function tn11(n) local acc, c = 0, 0 for i = 1, n do local v = tonumber(mixed[i % 8 + 1]) acc = acc + (v and 1 or 0) c = c + (v or 0) end return acc, c end
P(tn11(1600))
-- table.move: another table, the same table overlapping in both directions, the destination absent / nil, element kinds, destinations that grow,
-- counts above the unrolled limit, arguments that change in the middle of the loop, metatables, errors
local function dump(t, n) local r = {} for i = 1, n or #t do r[#r + 1] = tostring(t[i]) end return table.concat(r, ",") end
local function m1(n) local acc = 0 local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = {} for i = 1, n do t[1] = i move(t, 1, 8, 1, u) acc = acc + #u + u[1] + u[8] end return acc, dump(u) end
P(m1(900))
local function m2(n) local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 } for i = 1, n do move(t, 2, 6, 3) move(t, 3, 7, 2) end return dump(t, 10) end
P(m2(901)) P(m2(900))
local function m3(n) local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 } for i = 1, n do move(t, 1, 5, 4) end return dump(t, 10) end
P(m3(701)) P(m3(700))
local function m4(n) local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 } for i = 1, n do move(t, 4, 8, 1) end return dump(t, 10) end
P(m4(701))
local function m5(n) local t, u = { 1, 2, 3, 4, 5 }, { 9, 9, 9, 9, 9, 9, 9, 9 } local acc = 0 for i = 1, n do local r = move(t, 1, 5, 2, u) acc = acc + (r == u and 1 or 0) + (move(t, 1, 3, 2, nil) == t and 1 or 0) end return acc, dump(t, 5), dump(u, 8) end
P(m5(600))
local function m6(n) local t = { 1.5, "s", true, false, {}, 6, 7, 8 } local u = { 0, 0, 0, 0, 0, 0, 0, 0 } for i = 1, n do move(t, 1, 8, 1, u) end local c = 0 for i = 1, 8 do if rawequal(t[i], u[i]) then c = c + 1 end end return c end
P(m6(500))
local function m7(n) local t = { 1, 2, nil, 4, nil, 6 } local u = { 9, 9, 9, 9, 9, 9 } for i = 1, n do move(t, 1, 6, 1, u) end return dump(u, 6) end
P(m7(500))
local function m8(n) local acc = 0 for i = 1, n do local t = { 1, 2, 3 } local u = {} move(t, 1, 3, 1, u) move(t, 1, 3, 4, u) acc = acc + #u + u[6] end return acc end
P(m8(600))
local function m9(n) local acc = 0 local t = {} for i = 1, 30 do t[i] = i end local u = {} for i = 1, n do move(t, 1, 20, 1, u) acc = acc + #u end return acc end
P(m9(500))
local function m10(n) local acc = 0 local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = {} for i = 1, n do local e = i == n // 2 and 3 or 8 move(t, 1, e, 1, u) acc = acc + #u end return acc end
P(m10(800))
local function m11(n) local acc = 0 local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 } for i = 1, n do local d = i < n // 2 and 1 or 3 move(t, 1, 5, d, u) acc = acc + u[1] + u[7] end return acc, dump(u, 10) end
P(m11(800))
local function m12(n) local acc = 0 local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = { 0, 0, 0, 0, 0, 0, 0, 0 } for i = 1, n do local dst = i == n // 2 and t or u move(t, 2, 6, 1, dst) acc = acc + u[1] end return acc, dump(t, 8), dump(u, 8) end
P(m12(800))
local function m12b(n) local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = { 0, 0, 0, 0, 0, 0, 0, 0 } local dsts = { u, t } for i = 1, n do move(t, 1, 5, 3, dsts[(i + 1) % 2 + 1]) end return dump(t, 8), dump(u, 8) end
P(m12b(800)) P(m12b(801))
local function m12c(n) local acc = 0 for i = 1, n do local t = { 1, 2, 3, 4, 5, 6, 7, 8 } local u = { 0, 0, 0, 0, 0, 0, 0, 0 } local dsts = { u, t } move(t, 1, 5, 3, dsts[(i + 1) % 2 + 1]) acc = acc + t[5] * 3 + t[4] + u[5] end return acc end
P(m12c(900)) P(m12c(901))
local function m13(n) local acc = 0 local log = {} local t = { 1, 2, 3, 4 } local u = setmetatable({}, { __newindex = function(tb, k, v) log[#log + 1] = k rawset(tb, k, v) end }) for i = 1, n do move(t, 1, 4, 1, u) acc = acc + #u end return acc, #log end
P(m13(500))
local function m14(n) local acc = 0 local t = setmetatable({ 1 }, { __index = function(_, k) return k * 10 end }) local u = {} for i = 1, n do move(t, 1, 4, 1, u) acc = acc + u[4] end return acc, dump(u, 4) end
P(m14(500))
local function m15(n) local acc = 0 local t = { 1, 2, 3, 4 } local u = {} for i = 1, n do if i == n // 2 then setmetatable(u, { __newindex = function(tb, k, v) rawset(tb, k, v * 2) end }) u[1] = nil u[2] = nil u[3] = nil u[4] = nil end move(t, 1, 4, 1, u) acc = acc + u[1] + u[4] end return acc end
P(m15(800))
P(pcall(move)) P(pcall(move, {})) P(pcall(move, {}, 1, 2)) P(pcall(move, 1, 1, 2, 3)) P(pcall(move, {}, 1, 2, 3, 4)) P(pcall(move, {}, 1.5, 2, 3)) P(pcall(move, {}, "1", "2", "1")) P(pcall(move, {}, math.mininteger, 0, 1)) P(pcall(move, {}, 1, 2, math.maxinteger))
P(#move({ 1, 2, 3 }, 1, 0, 1, {}), #move({ 1, 2, 3 }, 2, 1, 5, {}), dump(move({ 1, 2, 3 }, -1, 1, 3, { 7, 7, 7 }), 8), dump(move({ 1, 2, 3 }, 1, 3, 0), 3))
local function m16(n) local acc = 0 local t = { 1, 2, 3, 4 } local u = {} for i = 1, n do acc = acc + #move(t, 1, i == n - 2 and 0 or 4, 1, u) end return acc end
P(m16(500))
local function m17(n) local acc = 0 local t = { 1, 2, 3, 4 } for i = 1, n do local u = {} move(t, 1, 4, i % 3 + 1, u) acc = acc + (u[1] or 0) + (u[4] or 0) end return acc end
P(m17(600))
local function m18(n) local t = { "a", "b", "c", "d", "e" } local acc = 0 for i = 1, n do local u = {} move(t, 1, 5, 1, u) acc = acc + #u collectgarbage("step", 0) end return acc end
P(m18(300))
-- concatenated keys (bit 8): present, absent (nil, then stored), cycling, resizing tables, keys that are long strings, numbers in keys, metatables
local function ck1(n) local t = {} for j = 1, 20 do t["key" .. j] = j end local acc = 0 for i = 1, n do acc = acc + t["key" .. (i % 20 + 1)] end return acc end
P(ck1(1200))
local function ck2(n) local t = {} for j = 1, 10 do t["key" .. j] = j end local acc, miss = 0, 0 for i = 1, n do local v = t["key" .. (i % 14 + 1)] if v then acc = acc + v else miss = miss + 1 end end return acc, miss end
P(ck2(1200))
local function ck3(n) local t = {} for i = 1, n do t["k" .. i] = i end local c = 0 for _ in pairs(t) do c = c + 1 end return c, t.k1, t["k" .. n] end
P(ck3(700))
local function ck4(n) local t = {} local acc = 0 for i = 1, n do local k = "w" .. (i % 7) t[k] = (t[k] or 0) + i acc = acc + t[k] end return acc, t.w0, t.w6 end
P(ck4(900))
local function ck5(n) local t = { ["a" .. ("x"):rep(50)] = 1, b1 = 2 } local acc = 0 for i = 1, n do acc = acc + (t["a" .. ("x"):rep(50)] or 0) + (t["b" .. 1] or 0) end return acc end
P(ck5(500))
local function ck6(n) local t = {} for j = 1, 8 do t[j .. "x"] = j end local acc = 0 for i = 1, n do acc = acc + (t[(i % 8 + 1) .. "x"] or 0) + (t[(i % 3 + 0.5) .. "x"] or 0) end return acc end
P(ck6(800))
local function ck7(n) local t = setmetatable({}, { __index = function(_, k) return #k end }) local acc = 0 for i = 1, n do acc = acc + t["kk" .. i % 10] end return acc end
P(ck7(600))
local function ck8(n) local log = {} local t = setmetatable({}, { __newindex = function(tb, k, v) log[#log + 1] = k rawset(tb, k, v) end }) for i = 1, n do t["n" .. i % 5] = i end return #log, t.n0, t.n4 end
P(ck8(600))
local function ck9(n) local t = {} for j = 1, 30 do t["z" .. j] = j end local acc = 0 for i = 1, n do local k = "z" .. (i % 30 + 1) acc = acc + t[k] if i % 100 == 0 then t[k] = nil end end return acc end
P(pcall(ck9, 900))
local function ck10(n) local t = {} local acc = 0 for i = 1, n do t["r" .. i % 40] = i if i % 50 == 0 then t = {} end acc = acc + (t["r" .. (i + 3) % 40] or -1) end return acc end
P(ck10(1000))
local function ck11(n) local acc = 0 local t = { s1 = 1, s2 = 2, s3 = 3 } for i = 1, n do local k = i % 3 == 0 and "s" .. 1 or ("s" .. (i % 3)) acc = acc + t[k] end return acc end
P(ck11(900))
print(table.concat(out, "\n"))
"####,
    },
    Prog {
        name: "bv_mm",
        small: true,
        src: r####"
-- BUNDLE-V (t1q bits 4 / 5): arithmetic metamethod frames in tier 1, tail calls and general returns in metamethod frames
local log = {}
local function note(...)
  local p = {}
  for i = 1, select("#", ...) do p[i] = tostring((select(i, ...))) end
  log[#log + 1] = table.concat(p, ":")
end
local V = {}
V.__index = V
local function new(x, y) return setmetatable({ x = x, y = y }, V) end
-- 1. every arithmetic / bitwise operator, table on the left, on the right, constants (MMBINI / MMBINK)
V.__add = function(a, b) if type(a) == "number" then return new(a + b.x, b.y) elseif type(b) == "number" then return new(a.x + b, a.y) end return new(a.x + b.x, a.y + b.y) end
V.__sub = function(a, b) if type(b) == "number" then return new(a.x - b, a.y - b) end return new(a.x - b.x, a.y - b.y) end
V.__mul = function(a, b) if type(a) == "number" then return new(a * b.x, a * b.y) elseif type(b) == "number" then return new(a.x * b, a.y * b) end return a.x * b.x + a.y * b.y end
V.__div = function(a, b) return new(a.x / b, a.y / b) end
V.__mod = function(a, b) return new(a.x % b, a.y % b) end
V.__idiv = function(...) local a, b = ...; local r; for _ = 1, 2 do r = new(a.x // b, a.y // b) end; return r end
V.__pow = function(a, b) return new(a.x ^ b, a.y ^ b) end
V.__unm = function(a) return new(-a.x, -a.y) end
V.__band = function(a, b) return (type(a) == "table" and a.x or a) & (type(b) == "table" and b.x or b) end
V.__bor = function(a, b) return (type(a) == "table" and a.x or a) | (type(b) == "table" and b.x or b) end
V.__bxor = function(a, b) return (type(a) == "table" and a.x or a) ~ (type(b) == "table" and b.x or b) end
V.__shl = function(a, b) return (type(a) == "table" and a.x or a) << (type(b) == "table" and b.x or b) end
V.__shr = function(a, b) return (type(a) == "table" and a.x or a) >> (type(b) == "table" and b.x or b) end
V.__bnot = function(a) return ~a.x end
do
  local function f(n)
    local a, b = new(1, 2), new(3, 4)
    local s = 0
    for i = 1, n do
      local c = a + b
      local d = c - a
      local e = d * 2
      local g = 3 * e
      local h = a * b
      local m = -c
      local q = c / 2
      local r = c % 3
      local u = c // 2
      local w = a ^ 2
      local k = a + 1
      local k2 = 1 + a
      local k3 = a - 1.5
      local k4 = a * 2.5
      s = s + c.x + d.y + e.x + g.y + h + m.x + q.y + r.x + u.y + w.x + k.x + k2.y + k3.x + k4.y
      s = s + (a & 3) + (3 | a) + (a ~ b) + (a << 1) + (a >> 1) + (~a) + (a & b)
    end
    return s
  end
  note("arith", f(300), f(50))
end
-- 2. handlers that tail call: a Lua function, a vararg Lua function, a native, a __call table, a __call chain, a C function that returns many
do
  local W = {}
  local function tl(a, b) return new(a.x + b.x, a.y + b.y) end
  local function tv(...) local n = select("#", ...) return new(n, n) end
  local callable = setmetatable({}, { __call = function(self, a, b) return new(a.x * 2, b.y * 2) end })
  local chain = setmetatable({}, { __call = setmetatable({}, { __call = function(_, self, a, b) return new(7, 8) end }) })
  W.__add = function(a, b) return tl(a, b) end
  W.__sub = function(a, b) return tv(a, b, 1, 2, 3) end
  W.__mul = function(a, b) return callable(a, b) end
  W.__div = function(a, b) return chain(a, b) end
  W.__mod = function(a, b) return select(2, a, b) end
  W.__idiv = function(a, b) return math.max(a.x, b.x) end
  local ctype = setmetatable({}, { __call = type })
  W.__pow = function(a, b) return ctype(a, b) end
  W.__unm = function(a) return setmetatable({ x = -a.x, y = -a.y }, V) end
  W.__band = function(a, b) return (function(...) return ... end)(a.x, b.x, 99) end
  W.__bor = function(a, b) return select("#", a, b) end
  W.__bxor = function(a, b) return rawequal(a, b) end
  W.__shl = function(a, b) return table.unpack({ a.x, b.x }) end
  W.__shr = function(a, b) return next({}) end
  local function mkw(x, y) return setmetatable({ x = x, y = y }, W) end
  local function g(n)
    local a, b = mkw(1, 2), mkw(3, 4)
    local s = 0
    for i = 1, n do
      local c1, c2, c3, c4, c5 = a + b, a - b, a * b, a / b, a % b
      local c6, c7, c8, c9, c10 = a // b, a ^ b, -a, a & b, a | b
      local c11, c12, c13 = a ~ b, a << b, a >> b
      s = s + c1.x + c2.x + c3.x + c4.x + tostring(c5):len() + c6 + #c7 + c8.x + c9 + c10 + (c11 and 1 or 0) + c12 + (c13 == nil and 1 or 0)
    end
    return s
  end
  note("tail", g(300), g(40))
end
-- 3. handler returning 0 / 1 / many values / wrong types, vararg handlers with extra args
do
  local mode = 0
  local T = {}
  T.__add = function(a, b)
    if mode == 0 then return elseif mode == 1 then return 1 elseif mode == 2 then return 1, 2, 3 elseif mode == 3 then return nil, 5 elseif mode == 4 then return "s" elseif mode == 5 then return a, b
    else return { n = mode } end
  end
  T.__unm = function(...) return select("#", ...), ... end
  T.__concat = function(a, b, ...) return "cc" .. select("#", ...) end
  local function h(n)
    local a, b = setmetatable({}, T), setmetatable({}, T)
    local s = {}
    for i = 1, n do
      mode = i % 7
      local r = a + b
      s[#s + 1] = type(r) == "table" and (r.n or "tbl") or tostring(r)
      local u = -a
      s[#s + 1] = tostring(u)
      s[#s + 1] = a .. "x"
    end
    return table.concat(s, ",")
  end
  note("rets", #h(70), h(14))
end
-- 4. errors in handlers: message, traceback, line info, error in the tail callee, non-string error values
do
  local E = {}
  local function boom(x) error("boom" .. tostring(x)) end
  E.__add = function(a, b) return boom(a.v) end
  E.__sub = function(a, b) error({ code = a.v }) end
  E.__mul = function(a, b) local x = nil; return x.y end
  E.__div = function(a, b) return a.v / "z" end
  E.__unm = function(a) error("unm", 2) end
  local function k(n)
    local a, b = setmetatable({ v = 3 }, E), setmetatable({ v = 4 }, E)
    local out = {}
    for i = 1, n do
      local ok, e = pcall(function() return a + b end)
      out[#out + 1] = tostring(e)
      ok, e = pcall(function() return a - b end)
      out[#out + 1] = type(e) == "table" and e.code or tostring(e)
      ok, e = pcall(function() return a * b end)
      out[#out + 1] = tostring(e)
      ok, e = pcall(function() return a / b end)
      out[#out + 1] = tostring(e)
      ok, e = pcall(function() return -a end)
      out[#out + 1] = tostring(e)
      local ok2, tb = xpcall(function() return a + b end, debug.traceback)
      out[#out + 1] = #tb > 0 and select(2, tb:gsub("\n", "\n")) or 0
    end
    return out
  end
  local o = k(40)
  note("err", o[1], o[2], o[3], o[4], o[5], o[6], #o)
  local ok, tb = xpcall(function() local a = setmetatable({ v = 1 }, E) return a + a end, debug.traceback)
  note("tb", tb)
end
-- 5. debug library from the handler: getinfo, getlocal, traceback, sethook
do
  local D = {}
  local seen = {}
  D.__add = function(a, b)
    local info = debug.getinfo(1, "nSl")
    local caller = debug.getinfo(2, "l")
    local nm, val = debug.getlocal(1, 1)
    seen[#seen + 1] = (info.name or "?") .. ":" .. tostring(info.currentline - info.linedefined) .. ":" .. tostring(caller.currentline ~= nil) .. ":" .. tostring(nm) .. ":" .. tostring(val == a)
    return debug.traceback("x", 1):len() > 0 and a or b
  end
  D.__unm = function(a)
    debug.sethook(function() end, "l")
    debug.sethook()
    return a
  end
  local function d(n)
    local a, b = setmetatable({}, D), setmetatable({}, D)
    local c = 0
    for i = 1, n do
      local r = a + b
      if r == a then c = c + 1 end
      r = -a
      if r == a then c = c + 1 end
    end
    return c
  end
  note("dbg", d(120), #seen, seen[1], seen[#seen])
  local hits = 0
  local H = { __add = function(a, b) debug.sethook(function() hits = hits + 1 end, "l"); return a end }
  local function hh(n) local a = setmetatable({}, H) for i = 1, n do local r = a + a end debug.sethook() return hits > 0 end
  note("hook", hh(30))
end
-- 6. yields: inside the handler, through a tail-called coroutine.yield, in coroutines sharing a table
do
  local Y = {}
  Y.__add = function(a, b) return coroutine.yield(a.v + b.v) end
  Y.__sub = function(a, b) local r = coroutine.yield("sub") return r end
  Y.__unm = function(a) return (coroutine.yield("unm")) end
  local function yy(n)
    local a, b = setmetatable({ v = 1 }, Y), setmetatable({ v = 2 }, Y)
    local s = 0
    for i = 1, n do
      s = s + (a + b) + (a - b) + (-a)
    end
    return s
  end
  local co = coroutine.create(yy)
  local ok, v = coroutine.resume(co, 60)
  local n = 0
  while coroutine.status(co) == "suspended" do n = n + 1; ok, v = coroutine.resume(co, n % 5) end
  note("yield", ok, v, n)
  local cos = {}
  for j = 1, 3 do cos[j] = coroutine.create(function() return yy(20 + j) end) end
  local res = {}
  for step = 1, 200 do
    for j = 1, 3 do
      if coroutine.status(cos[j]) == "suspended" then local _, r = coroutine.resume(cos[j], step % 3); if r ~= nil and step > 190 then res[#res + 1] = tostring(r) end end
    end
  end
  note("yield2", #res)
end
-- 7. the metatable / handler mutated while the handler runs
do
  local M = {}
  local calls = 0
  M.__add = function(a, b)
    calls = calls + 1
    if calls == 50 then M.__add = function(a, b) return new(-1, -1) end end
    if calls == 80 then setmetatable(a, nil) end
    return new(a.x, b.y)
  end
  local function mm(n)
    local a, b = setmetatable({ x = 1, y = 2 }, M), setmetatable({ x = 3, y = 4 }, M)
    local s, err = 0, 0
    for i = 1, n do
      local ok, r = pcall(function() return a + b end)
      if ok then s = s + r.x + r.y else err = err + 1 end
      if i == 90 then a = setmetatable({ x = 5, y = 6 }, M) end
    end
    return s, err, calls
  end
  note("mut", mm(130))
end
-- 8. deep chains: a handler that adds again, runaway recursion
do
  local C = {}
  C.__add = function(a, b) if a.d <= 0 then return setmetatable({ d = 0 }, C) end return setmetatable({ d = 1 + (setmetatable({ d = a.d - 1 }, C) + b).d }, C) end
  local function dc(n, d) local s = 0 for i = 1, n do s = s + (setmetatable({ d = d }, C) + setmetatable({ d = 0 }, C)).d end return s end
  note("deep", dc(30, 20), dc(10, 100))
  local R = {}
  R.__add = function(a, b) return a + b end
  local ok, e = pcall(function() local a = setmetatable({}, R) for i = 1, 3 do local c = a + a end end)
  note("runaway", ok, (tostring(e):gsub("^.-:%d+: ", "")))
  local R2 = {}
  R2.__add = function(a, b) return (a + b) end
  local ok2, e2 = pcall(function() local a = setmetatable({}, R2) for i = 1, 3 do local c = a + a end end)
  note("runaway2", ok2, (tostring(e2):gsub("^.-:%d+: ", "")))
end
-- 9. comparisons with tail calls and mixed operands, __len, __concat, __eq
do
  local L = {}
  local function cmpf(a, b) return a.v < b.v end
  L.__lt = function(a, b) return cmpf(a, b) end
  L.__le = function(a, b) return select(1, a.v <= b.v, 1) end
  L.__eq = function(a, b) return rawequal(a, b) or a.v == b.v end
  L.__len = function(a) return a.v end
  L.__concat = function(a, b) return (type(a) == "table" and a.v or a) .. "|" .. (type(b) == "table" and b.v or b) end
  local function mk2(v) return setmetatable({ v = v }, L) end
  local function cc(n)
    local a, b, c = mk2(1), mk2(2), mk2(1)
    local s = {}
    for i = 1, n do
      local t = (a < b and 1 or 0) + (a <= b and 1 or 0) + (b < a and 1 or 0) + (a == c and 1 or 0) + (a ~= b and 1 or 0) + #a + #b
      local u = a .. b
      local w = "x" .. a .. 5 .. b
      local mixed = 1 .. a
      if i % 40 == 0 then s[#s + 1] = t .. u .. w .. mixed end
    end
    local ok, e = pcall(function() return a < 1 end)
    s[#s + 1] = tostring(ok) .. tostring(e):gsub("^.-:%d+: ", "")
    local ok3, e3 = pcall(function() return a < "x" end)
    s[#s + 1] = tostring(ok3)
    return table.concat(s, ",")
  end
  note("cmp", cc(120))
  local B = { __len = function() return 1.5 end, __lt = function() return nil end, __le = function() return 0 end }
  local bb = setmetatable({}, B)
  local function bl(n) local s = 0 for i = 1, n do s = s + #bb + ((bb < bb) and 1 or 0) + ((bb <= bb) and 1 or 0) end return s end
  note("badlen", bl(50))
end
-- 10. the vararg __call constructor (oop_mixins shape): __call tail calls, constructor with varargs
do
  local Obj = {}
  Obj.__index = Obj
  function Obj:new(...) self.n = select("#", ...); self.first = ... end
  Obj.__call = function(cls, ...) local o = setmetatable({}, cls); o:new(...); return o end
  local Vec2 = setmetatable({}, { __index = Obj, __call = Obj.__call })
  Vec2.__index = Vec2
  Vec2.__add = function(a, b) return Vec2(a.first + b.first, 0) end
  Vec2.__sub = function(a, b) return Vec2(a.first - b.first) end
  Vec2.__unm = function(a) return Vec2(-a.first, 1, 2, 3) end
  local function vv(n)
    local a, b = Vec2(1, 2), Vec2(10)
    local s = 0
    for i = 1, n do
      local c = a + b
      local d = c - a
      local e = -d
      s = s + c.first + d.first + e.first + e.n + c.n + d.n
    end
    return s
  end
  note("vcall", vv(300), vv(10))
end
-- 11. upvalues captured by handlers and closures created by handlers (open upvalues closed on return)
do
  local U = {}
  local count = 0
  U.__add = function(a, b) local k = count; count = count + 1; return function() k = k + 1; return k end end
  U.__unm = function(a) local z = a.z return setmetatable({ get = function() return z end }, U) end
  local function uu(n)
    local a = setmetatable({ z = 5 }, U)
    local s = 0
    for i = 1, n do
      local f = a + a
      s = s + f() + f()
      local g = -a
      s = s + g.get()
    end
    return s, count
  end
  note("upv", uu(100))
end
-- 12. native tail callees in __index / __newindex frames (refused by the tier-1 tail call: nothing may change, the count is given back) and in arithmetic frames
do
  local d = { a = 1, b = 2 }
  local P = setmetatable({}, { __index = function(_, k) return rawget(d, k) end, __newindex = function(t, k, v) return rawset(d, k, v) end })
  local function p(n) local s = 0 for i = 1, n do s = s + (P.a or 0) + (P.zz or 7); P.q = i; s = s + d.q end return s end
  note("natidx", p(300), p(20))
  local A = {}
  A.__add = function(a, b) return rawequal(a, b) end
  A.__lt = function(a, b) return rawequal(a, b) end
  A.__unm = function(a) return tostring(a.v) end
  local function ar(n) local a, b = setmetatable({ v = 3 }, A), setmetatable({ v = 4 }, A) local s = 0 for i = 1, n do if (a + b) then s = s + 1 end if a < b then s = s + 2 end if a < a then s = s + 4 end s = s + #(-a) end return s end
  note("natarith", ar(300), ar(20))
end
for i = 1, #log do print(log[i]) end
"####,
    },
    Prog {
        name: "bv_upbar",
        small: true,
        src: r####"
-- BUNDLE-V V2 (`upbar`): SETUPVAL of collectable values (and nil / false / numbers) into real upvalue cells is recorded (USTORE + the upvalue write barrier)
local log = {}
local function note(...)
  local p = {}
  for i = 1, select("#", ...) do p[i] = tostring((select(i, ...))) end
  log[#log + 1] = table.concat(p, ":")
end
-- 0. the minimal shapes: a closed upvalue and an open one, a fresh table stored every iteration (allocation drives the collector)
do
  local function mk() local a = { 0 } return function(v) a = v return a end end
  local f = mk()
  local function run(n) local s = 0 for i = 1, n do local t = f({ i }) s = s + t[1] end return s end
  note("closed0", run(2000), run(300))
  local cur = { 0 }
  local function set(v) cur = v end
  local function run2(n) local s = 0 for i = 1, n do set({ i, i }) s = s + cur[2] end return s end
  note("open0", run2(2000), run2(300))
  local x, y = { 1 }, { 2 }
  local function swap() x, y = y, x end
  local function run3(n) local s = 0 for i = 1, n do swap() y = { i } s = s + x[1] + y[1] end return s end
  note("swap0", run3(2000), run3(300))
end
-- 1. an LRU-like doubly linked list: head / tail upvalues stored from closures called in a loop
do
  local head, tail, size = nil, nil, 0
  local function unlink(n)
    if n.prev then n.prev.next = n.next else head = n.next end
    if n.next then n.next.prev = n.prev else tail = n.prev end
    n.prev, n.next = nil, nil
    size = size - 1
  end
  local function push(n)
    n.next = head
    if head then head.prev = n end
    head = n
    if not tail then tail = n end
    size = size + 1
  end
  local function run(n)
    local s = 0
    for i = 1, n do
      push({ k = i, v = { i } })
      if size > 5 then local t = tail; unlink(t) end
      if i % 3 == 0 and head.next then local m = head.next; unlink(m); push(m) end
      s = s + head.k + (tail and tail.k or 0) + #head.v
    end
    return s, size
  end
  note("lru", run(400))
  collectgarbage("step", 0)
  note("lru2", run(200))
end
-- 2. the stored values: tables, strings, functions, nil, false, numbers, coroutines, userdata-like (io files); the loop checks the contents afterwards
do
  local cur
  local function set(v) cur = v end
  local function run(n)
    local acc = 0
    for i = 1, n do
      local m = i % 8
      if m == 0 then set({ i, i + 1 })
      elseif m == 1 then set("s" .. i)
      elseif m == 2 then set(function() return i end)
      elseif m == 3 then set(nil)
      elseif m == 4 then set(false)
      elseif m == 5 then set(i * 1.5)
      elseif m == 6 then set(coroutine.create(function() end))
      else set(io.stdout) end
      local t = type(cur)
      acc = acc + #t
      if t == "table" then acc = acc + cur[1] + cur[2] elseif t == "string" then acc = acc + #cur elseif t == "function" then acc = acc + cur() end
    end
    return acc
  end
  note("kinds", run(300), run(100))
  collectgarbage()
end
-- 3. an open upvalue that aliases a register of the loop's own frame (the trace keeps it in a register), stored with a table
do
  local function run(n)
    local slot = { 0 }
    local function put(v) slot = v end
    local s = 0
    for i = 1, n do
      put({ i, { i * 2 } })
      s = s + slot[1] + slot[2][1]
    end
    return s
  end
  note("alias", run(200), run(30))
end
-- 4. an open upvalue shared between coroutines, then closed
do
  local function mk()
    local shared = { 0 }
    local function bump(v) shared = v end
    local function get() return shared end
    return bump, get
  end
  local bump, get = mk()
  local function worker(id, n)
    local s = 0
    for i = 1, n do
      bump({ id * 1000 + i })
      s = s + get()[1]
      if i % 6 == 0 then coroutine.yield(s) end
    end
    return s
  end
  local cos = {}
  for j = 1, 3 do cos[j] = coroutine.create(worker) end
  local last = {}
  for round = 1, 60 do
    for j = 1, 3 do
      if coroutine.status(cos[j]) == "suspended" then
        local ok, r = coroutine.resume(cos[j], j, 40)
        last[j] = r
      end
    end
  end
  note("coros", last[1], last[2], last[3], get()[1])
end
-- 5. closed upvalues (the function that created them has returned) stored in loops
do
  local function mk()
    local a, b = nil, {}
    return function(v) a, b = b, v return a end
  end
  local f = mk()
  local function run(n)
    local s = 0
    for i = 1, n do
      local old = f({ i })
      s = s + #old + (old[1] or 0)
    end
    return s
  end
  note("closed", run(300), run(90))
end
-- 6. a store inside a metamethod and a deep call chain; values that die right after the store
do
  local keep
  local mt = { __add = function(a, b) keep = { a.v + b.v } return keep end }
  local function run(n)
    local a, b = setmetatable({ v = 1 }, mt), setmetatable({ v = 2 }, mt)
    local s = 0
    for i = 1, n do
      local r = a + b
      s = s + r[1] + keep[1]
      keep = nil
      keep = { s }
    end
    return s
  end
  note("mmstore", run(150))
end
for i = 1, #log do print(log[i]) end
"####,
    },
];

/// Y5: programs only the dedicated tests run (not in `selected()`: they build a large heap and are meaningless under the generic matrices).
pub const Y_PROGS: &[Prog] = &[
    Prog {
        name: "y5_upbar_black",
        small: false,
        src: concat!("local KEEP, N, ROUNDS = 40000, 2000, 40\n", r####"-- Y5: a trace stores a fresh (white) table into a closed upvalue cell that the collector has already marked (black).
-- Incremental: the heap is large, so the mark phase spans many steps (each loop iteration allocates and pays debt).
-- Generational: the cell and its closure become old; the stored table is young (the forward barrier of OP_SETUPVAL).
local function mk()
  local c = false
  return function(t) c = t end, function() return c end
end
local set, get = mk()
local set2, get2 = mk()
local keep = {}
for i = 1, KEEP do keep[i] = { i, tostring(i % 10) } end
local acc = 0
local function run(n)
  local s = 0
  for i = 1, n do
    set({ i, i + 1 })
    local t = get()
    s = s + t[1] + t[2]
    set2(t)
    s = s + get2()[1]
  end
  return s
end
for r = 1, ROUNDS do
  acc = (acc + run(N)) % 1000003
  if r % 4 == 0 then collectgarbage("step", 0) end
  if r % 10 == 0 then keep[(r * 7) % KEEP + 1] = { r } end
end
print(acc, get()[1], get2()[2], #keep)
"####),
    },
    Prog {
        name: "y5_upbar_black_small",
        small: true,
        src: concat!("local KEEP, N, ROUNDS = 300, 60, 16\n", r####"-- Y5: a trace stores a fresh (white) table into a closed upvalue cell that the collector has already marked (black).
-- Incremental: the heap is large, so the mark phase spans many steps (each loop iteration allocates and pays debt).
-- Generational: the cell and its closure become old; the stored table is young (the forward barrier of OP_SETUPVAL).
local function mk()
  local c = false
  return function(t) c = t end, function() return c end
end
local set, get = mk()
local set2, get2 = mk()
local keep = {}
for i = 1, KEEP do keep[i] = { i, tostring(i % 10) } end
local acc = 0
local function run(n)
  local s = 0
  for i = 1, n do
    set({ i, i + 1 })
    local t = get()
    s = s + t[1] + t[2]
    set2(t)
    s = s + get2()[1]
  end
  return s
end
for r = 1, ROUNDS do
  acc = (acc + run(N)) % 1000003
  if r % 4 == 0 then collectgarbage("step", 0) end
  if r % 10 == 0 then keep[(r * 7) % KEEP + 1] = { r } end
end
print(acc, get()[1], get2()[2], #keep)
"####),
    },
    Prog {
        name: "y2_insert_storm",
        small: true,
        src: r####"
-- Y2: `table.insert(u, v)` and `u[#u + 1] = v` are `TLEN` + `TSET` in a trace; an exit storm must not fail the guard behind the `TSET` (it fails only
-- when nothing was stored): the exit resumes at the `CALL` and the element was stored twice (`#u` 752 instead of 750).
local function fill(n)
  local u = {}
  for i = 1, n do table.insert(u, i) end
  return u
end
local function fill2(n)
  local u = {}
  for i = 1, n do u[#u + 1] = i * 2 end
  return u
end
local function drain(u, n)
  local s = 0
  for i = 1, n do s = s + table.remove(u) end
  return s
end
local a, b = fill(750), fill2(750)
print(#a, #b, a[750], b[750], drain(a, 375), #a)
local u = {}
for i = 1, 500 do table.insert(u, i) end
for i = 1, 200 do table.insert(u, 1, i); table.remove(u, 1) end
print(#u, u[1], u[500])
"####,
    },
    Prog {
        name: "va_tmove_overlap",
        small: true,
        src: r####"
-- BUNDLE-VA: a `table.move` over a range of ONE table (overlapping: the sources are overwritten by the stores) whose type guard fails between two
-- element copies. The trace unrolls the move as interleaved element loads and stores; the exit resumes at the `CALL`, which the interpreter runs
-- again from the start over the half-moved elements. The mark (a float or a string among integers) travels through the range from one
-- iteration to the next, so the loads of a hot trace meet a type they were not recorded with in the middle of a move.
local move = table.move
local function dump(t, n) local r = {} for i = 1, n do r[i] = tostring(t[i]) end return table.concat(r, ",") end
local mark = { 0.5, "m", true }
-- copy order backwards (dest > first, dest <= last): `move(t, 2, 6, 3)`
local function rev(t, rounds) for i = 1, rounds do move(t, 2, 6, 3) end end
-- copy order forwards (dest <= first): `move(t, 3, 7, 2)`
local function fwd(t, rounds) for i = 1, rounds do move(t, 3, 7, 2) end end
-- the same with a second statement after the move (two moves per iteration)
local function both(t, rounds) for i = 1, rounds do move(t, 2, 6, 3) move(t, 3, 7, 2) end end
local out = {}
for rep = 1, 40 do
  for m = 1, #mark do
    for pos = 2, 8 do
      for _, f in ipairs { rev, fwd, both } do
        local t = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 }
        t[pos] = mark[m]
        f(t, 3 + rep % 5)
        out[#out + 1] = dump(t, 10)
      end
    end
  end
end
local h = 0
for i = 1, #out do for c = 1, #out[i] do h = (h * 31 + out[i]:byte(c)) % 1000003 end end
print(#out, out[1], out[#out // 2], out[#out], h)
"####,
    },
];
