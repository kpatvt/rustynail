//! H1 programs (`hkaddr`: the `HREFK` address is folded into its `HLOAD` / `HSTORE` users) and C1 programs (the callee identity guard of
//! a call through an upvalue / a table field). Each keeps its loops inside functions and prints values that expose a wrong displacement
//! (another field's value), a stale callee or a missing exit.

use super::progs::Prog;

pub const H1_PROGS: &[Prog] = &[
    Prog {
        name: "h1_field_positions",
        small: false,
        src: r#"
-- many fields of one table (different node slots, collision chains), loaded and stored through constant keys in a hot loop
local function f(t, n)
  local s = 0
  for i = 1, n do
    s = s + t.a + t.b * 2 + t.c * 3 + t.d * 4 + t.e * 5 + t.f * 6 + t.g * 7 + t.h * 8 + t.i * 9 + t.j * 10 + t.k * 11 + t.l * 12
    t.a = t.a + 1
    t.f = t.f + t.a
    t.l = t.l - 1
    t.c = t.b + t.c
    t.h = i
  end
  return s, t.a, t.b, t.c, t.d, t.e, t.f, t.g, t.h, t.i, t.j, t.k, t.l
end
local function mk(off)
  return { a = 1 + off, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9, j = 10, k = 11, l = 12 }
end
local out = {}
for r = 1, 30 do f(mk(0), 40) end
for r = 1, 3 do out[#out + 1] = table.concat({ f(mk(r), 50) }, ",") end
-- a table with the same fields in another insertion order (other node positions): guards fail, side traces take over
local function mk2()
  local t = {}
  for _, k in ipairs { "l", "k", "j", "i", "h", "g", "f", "e", "d", "c", "b", "a" } do t[k] = #k + string.byte(k) % 7 end
  return t
end
for r = 1, 30 do out[#out + 1] = table.concat({ f(mk2(), 20) }, ",") end
-- float and integer fields mixed, a field turned into a float mid-run
local t = mk(0)
for r = 1, 20 do f(t, 10) end
t.d = 0.5
out[#out + 1] = table.concat({ f(t, 10) }, ",")
t.d = "7"
out[#out + 1] = tostring(pcall(f, t, 10))
t.d = nil
out[#out + 1] = tostring(pcall(f, t, 10))
t.d = 4
out[#out + 1] = table.concat({ f(t, 10) }, ",")
-- extra keys that rehash the node vector
for k = 1, 40 do t["x" .. k] = k end
out[#out + 1] = table.concat({ f(t, 10) }, ",")
for k = 1, 40 do t["x" .. k] = nil end
out[#out + 1] = table.concat({ f(t, 10) }, ",")
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "h1_index_chain",
        small: false,
        src: r#"
-- fields found through __index tables (the HREFK is on the class table) and through own fields; instances change class mid-run
local A = { x = 1, y = 2, z = 3 }
A.__index = A
local B = { x = 10, y = 20, z = 30, w = 5 }
B.__index = B
local function f(o, n)
  local s = 0
  for i = 1, n do
    s = s + o.x + o.y * 2 + o.z * 3
    o.cnt = (o.cnt or 0) + 1
  end
  return s, o.cnt
end
local out = {}
local a = setmetatable({}, A)
for r = 1, 40 do f(a, 20) end
for r = 1, 3 do out[#out + 1] = table.concat({ f(a, 20) }, ",") end
local b = setmetatable({ cnt = 0 }, B)
for r = 1, 3 do out[#out + 1] = table.concat({ f(b, 20) }, ",") end
a.x = 100
out[#out + 1] = table.concat({ f(a, 20) }, ",")
setmetatable(a, B)
out[#out + 1] = table.concat({ f(a, 20) }, ",")
B.y = 21
out[#out + 1] = table.concat({ f(a, 20) }, ",")
local c = setmetatable({}, { __index = function(_, k) return #k end })
out[#out + 1] = tostring(pcall(f, c, 5))
local d = setmetatable({}, { __index = { x = 1, y = 2, z = 3 }, __newindex = function(t, k, v) rawset(t, k, v * 2) end })
out[#out + 1] = table.concat({ f(d, 5) }, ",")
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "h1_nbody_like",
        small: false,
        src: r#"
-- nbody-like pair loop: fields of two bodies loaded and stored through constant keys; a body whose field order differs
local sqrt = math.sqrt
local function advance(bodies, n, dt)
  for _ = 1, n do
    for i = 1, #bodies do
      local bi = bodies[i]
      local bix, biy, biz, bimass = bi.x, bi.y, bi.z, bi.mass
      local bivx, bivy, bivz = bi.vx, bi.vy, bi.vz
      for j = i + 1, #bodies do
        local bj = bodies[j]
        local dx, dy, dz = bix - bj.x, biy - bj.y, biz - bj.z
        local d2 = dx * dx + dy * dy + dz * dz
        local mag = sqrt(d2); mag = dt / (mag * d2)
        local bm = bj.mass * mag
        bivx = bivx - dx * bm; bivy = bivy - dy * bm; bivz = bivz - dz * bm
        bm = bimass * mag
        bj.vx = bj.vx + dx * bm; bj.vy = bj.vy + dy * bm; bj.vz = bj.vz + dz * bm
      end
      bi.vx = bivx; bi.vy = bivy; bi.vz = bivz
      bi.x = bix + dt * bivx; bi.y = biy + dt * bivy; bi.z = biz + dt * bivz
    end
  end
end
local function energy(bodies)
  local e = 0
  for i = 1, #bodies do
    local b = bodies[i]
    e = e + 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz)
  end
  return e
end
local function mk(k, other)
  local b
  if other then b = { mass = 0.1 * k, vz = 0.01 * k, vy = 0.02, vx = 0.03, z = 3.0 - k, y = 2.0 + k, x = 1.0 * k }
  else b = { x = 1.0 * k, y = 2.0 + k, z = 3.0 - k, vx = 0.03, vy = 0.02, vz = 0.01 * k, mass = 0.1 * k } end
  return b
end
local out = {}
local bodies = {}
for k = 1, 5 do bodies[k] = mk(k, false) end
advance(bodies, 200, 0.01)
out[#out + 1] = string.format("%.12g", energy(bodies))
bodies[3] = mk(3, true)
advance(bodies, 100, 0.01)
out[#out + 1] = string.format("%.12g", energy(bodies))
bodies[2].vx = 1
advance(bodies, 50, 0.01)
out[#out + 1] = string.format("%.12g", energy(bodies))
bodies[4].mass = 3
advance(bodies, 50, 0.01)
out[#out + 1] = string.format("%.12g", energy(bodies))
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "h1_store_kinds",
        small: false,
        src: r#"
-- stores through folded addresses: different value types into one field, nil old values (insertion), rawset-style changes, metatables
local function f(t, n)
  for i = 1, n do
    t.a = t.a + 1
    t.b = t.a * 0.5
    t.c = i
    t.d = t.c
  end
  return t.a, t.b, t.c, t.d
end
local out = {}
local function mk() return { a = 0, b = 0.0, c = 0, d = 0 } end
for r = 1, 30 do f(mk(), 30) end
for r = 1, 3 do out[#out + 1] = table.concat({ f(mk(), 30) }, ",") end
local t = mk()
t.d = nil
out[#out + 1] = table.concat({ f(t, 30) }, ",")
local log = {}
local u = setmetatable(mk(), { __newindex = function(tt, k, v) log[#log + 1] = k; rawset(tt, k, v) end })
u.d = nil
out[#out + 1] = table.concat({ f(u, 30) }, ",") .. "|" .. table.concat(log, "")
local w = mk()
w.c = 1.5
out[#out + 1] = tostring(pcall(f, w, 5))
w.a = "1"
out[#out + 1] = tostring(pcall(f, w, 5))
local ro = setmetatable({}, { __index = mk(), __newindex = function(tt, k, v) rawset(tt, k, v) end })
out[#out + 1] = table.concat({ f(ro, 30) }, ",")
print(table.concat(out, " "))
"#,
    },
    // ---- C1: the callee identity guard of a call through an upvalue / a table field ----
    Prog {
        name: "c1_upvalue_callee",
        small: false,
        src: r#"
-- a loop calling an upvalue alias of math.sqrt: the alias is replaced between activations and the global itself is reassigned
local sqrt = math.sqrt
local function f(n, x)
  local s = 0.0
  for i = 1, n do s = s + sqrt(x + i) end
  return s
end
local function setsqrt(v) sqrt = v end
local out = {}
for r = 1, 40 do f(50, 2.0) end
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
setsqrt(function(v) return v * 2 end)
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
setsqrt(math.sqrt)
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
setsqrt(math.abs)
out[#out + 1] = string.format("%.10g", f(50, -2.0))
setsqrt(math.sqrt)
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
setsqrt(nil)
out[#out + 1] = (tostring(select(2, pcall(f, 50, 2.0))):gsub("^.-:%d+: ", ""))
setsqrt(setmetatable({}, { __call = function(_, v) return v + 1 end }))
out[#out + 1] = string.format("%.10g", f(50, 2.0))
setsqrt(math.sqrt)
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
local real = math.sqrt
math.sqrt = function(v) return -v end
for r = 1, 3 do out[#out + 1] = string.format("%.10g", f(50, 2.0)) end
math.sqrt = real
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "c1_callee_midloop",
        small: false,
        src: r#"
-- the callee upvalue changes in the middle of a hot loop (by the loop itself, by a called function, by debug.setupvalue)
local sq = math.sqrt
local function f(n, x)
  local s = 0.0
  for i = 1, n do
    s = s + sq(x + i)
    if i == 40 then sq = function(v) return v end end
    if i == 60 then sq = math.sqrt end
    if i == 80 then sq = math.floor end
    if i == 90 then sq = math.sqrt end
  end
  return s
end
local out = {}
for r = 1, 30 do f(30, 2.0) end
for r = 1, 4 do out[#out + 1] = string.format("%.10g", f(100, 2.0)) end
local function g(n, x)
  local s = 0.0
  for i = 1, n do
    s = s + sq(x + i)
    if i == 30 then debug.setupvalue(f, 1, function(v) return v * 3 end) end
  end
  return s
end
-- g and f share the upvalue sq (index 1 of f is sq: it is the only upvalue of f)
for r = 1, 3 do
  sq = math.sqrt
  out[#out + 1] = string.format("%.10g", g(60, 2.0))
end
sq = math.sqrt
for r = 1, 20 do g(20, 2.0) end
for r = 1, 3 do
  sq = math.sqrt
  out[#out + 1] = string.format("%.10g", g(60, 2.0))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "c1_open_closed",
        small: false,
        src: r#"
-- the callee alias is a local of an enclosing frame: open while that frame runs, closed afterwards; the closure is a fresh one per call
local function mk(callee)
  local fn = callee
  local function loop(n, x)
    local s = 0.0
    for i = 1, n do s = s + fn(x + i) end
    return s
  end
  local function set(v) fn = v end
  return loop, set
end
local out = {}
local l1, s1 = mk(math.sqrt)
for r = 1, 30 do l1(40, 2.0) end
out[#out + 1] = string.format("%.10g", l1(40, 2.0))
s1(function(v) return v + 0.5 end)
out[#out + 1] = string.format("%.10g", l1(40, 2.0))
local l2, s2 = mk(math.abs)
out[#out + 1] = string.format("%.10g", l2(40, -50.0))
for r = 1, 30 do l2(40, -50.0) end
out[#out + 1] = string.format("%.10g", l2(40, -50.0))
-- open: the loop runs inside the frame that owns the alias
local function outer(n)
  local c = math.sqrt
  local function inner(m, x)
    local s = 0.0
    for i = 1, m do
      s = s + c(x + i)
      if i == m // 2 then c = math.abs end
    end
    return s
  end
  local t = 0.0
  for k = 1, n do c = math.sqrt; t = t + inner(30, 2.0) end
  return t
end
for r = 1, 3 do out[#out + 1] = string.format("%.10g", outer(10)) end
-- two closures of the same prototype with different callees alternate
local la, sa = mk(math.sqrt)
local lb, sb = mk(function(v) return v * 2 end)
for r = 1, 30 do
  out[#out + 1] = string.format("%.6g %.6g", la(10, 1.0), lb(10, 1.0))
end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "c1_lua_callee",
        small: false,
        src: r#"
-- a Lua function callee (upvalue and global) replaced mid-loop, also with a vararg function and with a C function
local function add1(v) return v + 1 end
local function add2(v) return v + 2 end
local cal = add1
G = add1
local function f(n)
  local s = 0
  for i = 1, n do
    s = s + cal(i) + G(i)
    if i == 50 then cal = add2 end
    if i == 70 then G = add2 end
    if i == 80 then cal = add1; G = function(...) return select('#', ...) end end
    if i == 90 then G = math.abs end
  end
  return s
end
local out = {}
for r = 1, 30 do cal = add1; G = add1; f(40) end
for r = 1, 4 do cal = add1; G = add1; out[#out + 1] = f(100) end
cal = math.floor; G = tostring
out[#out + 1] = tostring(pcall(f, 30))
cal = add1; G = add1
for r = 1, 4 do out[#out + 1] = f(100); cal = add1; G = add1 end
print(table.concat(out, " "))
"#,
    },
    Prog {
        name: "h1b_int_absent",
        small: true,
        src: r##"
-- H1b (CP5): reads of integer keys that are absent (outside the array part, not in the hash part) of tables without a metatable, in
-- loops and in a recursion whose leaf is the absent read; the table changes under the trace: metatable with __index added later or
-- removed, array growth over the key, keys inserted into the hash part, rehash, float / zero / negative / huge keys.
local function rd(t, k) return t[k] end
local function item(t)
  if t[1] then return 1 + item(t[1]) + item(t[2]) end
  return 1
end
local function bottom(d)
  if d > 0 then return { bottom(d - 1), bottom(d - 1) } end
  return {}
end
local out = {}
local tot = 0
for i = 1, 40 do tot = tot + item(bottom(6)) end
out[#out + 1] = tot
-- a leaf that grows an element (the root of the next run is a node)
local function mix(n)
  local s = 0
  for i = 1, n do
    local t = {}
    if i % 5 == 0 then t[1] = i end
    if i % 7 == 0 then t[2] = i end
    if i % 11 == 0 then t[-1] = i end
    if i % 13 == 0 then t[1 << 40] = i end
    s = s + (rd(t, 1) or 0) + (rd(t, 2) or 0) + (rd(t, -1) or 0) + (rd(t, 1 << 40) or 0) + (rd(t, 0) or 0)
  end
  return s
end
for r = 1, 30 do mix(60) end
out[#out + 1] = mix(300)
-- one table: keys appear (hash insertion, array growth, rehash) and disappear between the reads
local function probe(t, n)
  local s = 0
  for i = 1, n do
    s = s + (rd(t, 5) or 0) + (rd(t, 6) or 0) + (rd(t, 1000) or 0) + (rd(t, -5) or 0)
  end
  return s
end
local t = {}
for r = 1, 40 do probe(t, 20) end
out[#out + 1] = probe(t, 50)
t[5] = 50
out[#out + 1] = probe(t, 50)
t[6] = 60; t[1000] = 1
out[#out + 1] = probe(t, 50)
for i = 1, 40 do t[i] = i end
out[#out + 1] = probe(t, 50)
for i = 1, 40 do t[i] = nil end
t[1000] = nil
out[#out + 1] = probe(t, 50)
t[-5] = 3
out[#out + 1] = probe(t, 50)
-- a metatable added later, with __index (table, function) and without; then removed
local mt = { __index = function(_, k) return k end }
setmetatable(t, mt)
out[#out + 1] = probe(t, 50)
setmetatable(t, { __index = { [5] = 1, [6] = 2, [1000] = 3, [-5] = 4 } })
out[#out + 1] = probe(t, 50)
setmetatable(t, {})
out[#out + 1] = probe(t, 50)
setmetatable(t, nil)
out[#out + 1] = probe(t, 50)
-- the same table, hot without a metatable, then a metatable with __index arrives between two calls of the compiled loop
local function hot(u, n)
  local s = 0
  for i = 1, n do s = s + (u[3] or 0) + (u[i + 100] or 0) end
  return s
end
local hu = {}
for r = 1, 60 do hot(hu, 30) end
out[#out + 1] = hot(hu, 30)
setmetatable(hu, { __index = function(_, k) return 1 end })
out[#out + 1] = hot(hu, 30)
setmetatable(hu, { __index = { [3] = 5 } })
out[#out + 1] = hot(hu, 30)
setmetatable(hu, nil)
out[#out + 1] = hot(hu, 30)
-- fresh tables, the metatable arrives at the same site in the middle of the loop
local function lf(n, m)
  local s = 0
  for i = 1, n do
    local u = {}
    if i > m then setmetatable(u, mt) end
    s = s + (rd(u, 3) or 0) + (rd(u, 4.0) or 0)
  end
  return s
end
for r = 1, 30 do lf(40, 1000) end
out[#out + 1] = lf(100, 60)
-- float keys: integral (normalised), fractional, huge
local fk = { 1.0, 2.5, 3.0, 1e300, -0.0, 0.5 }
local function fl(tt, n)
  local s = 0
  for i = 1, n do
    for j = 1, #fk do s = s + (rd(tt, fk[j]) or 0) end
  end
  return s
end
local ft = {}
for r = 1, 30 do fl(ft, 10) end
out[#out + 1] = fl(ft, 30)
ft[3] = 7; ft[0] = 5
out[#out + 1] = fl(ft, 30)
print(table.concat(out, " "))
"##,
    },
    Prog {
        name: "h1b_bintree",
        small: true,
        src: r##"
-- H1b (CP5): the binary_trees shape: the leaf `tree[1]` on `{}` is an absent integer key; the leaf exit of the recursion root gets a side trace
local function BottomUpTree(depth)
  if depth > 0 then depth = depth - 1; local left, right = BottomUpTree(depth), BottomUpTree(depth); return { left, right }
  else return {} end
end
local function ItemCheck(tree) if tree[1] then return 1 + ItemCheck(tree[1]) + ItemCheck(tree[2]) else return 1 end end
local out = {}
for depth = 4, 10, 2 do
  local check = 0
  for i = 1, 2 ^ (12 - depth) do check = check + ItemCheck(BottomUpTree(depth)) end
  out[#out + 1] = check
end
print(table.concat(out, " "))
"##,
    },
];
