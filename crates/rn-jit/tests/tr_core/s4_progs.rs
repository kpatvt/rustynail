//! S4 programs (`shapeguard=1`: the `HREFK`s of a table are covered by one guard on the layout id of its node vector, `HSHAPE`, and skip the
//! key compare). Each program reads and writes tables by constant field names in hot loops and prints values that expose a wrong node: a
//! key at another node of a table of a different layout, a field that was deleted and re-added, a table whose id was never computed, recycled
//! nodes after a collection, strings freed and re-created at the same address.

use super::progs::Prog;

pub const S4_PROGS: &[Prog] = &[
    Prog {
        name: "s4_nbody_like",
        small: false,
        src: r#"
-- objects built identically (seven fields: one layout, one id) and a loop that reads and writes them by field
local function mk(i)
  return { x = i, y = i * 2, z = i * 3, vx = 0.5, vy = 0.25, vz = 0.125, mass = i + 1.5 }
end
local bodies = {}
for i = 1, 5 do bodies[i] = mk(i) end
local function advance(n, dt)
  for _ = 1, n do
    for i = 1, #bodies do
      local bi = bodies[i]
      local bx, by, bz = bi.x, bi.y, bi.z
      for j = i + 1, #bodies do
        local bj = bodies[j]
        local dx, dy, dz = bx - bj.x, by - bj.y, bz - bj.z
        local d2 = dx * dx + dy * dy + dz * dz + 0.01
        local mag = dt / (d2 * math.sqrt(d2))
        bi.vx = bi.vx - dx * bj.mass * mag
        bi.vy = bi.vy - dy * bj.mass * mag
        bi.vz = bi.vz - dz * bj.mass * mag
        bj.vx = bj.vx + dx * bi.mass * mag
        bj.vy = bj.vy + dy * bi.mass * mag
        bj.vz = bj.vz + dz * bi.mass * mag
      end
    end
    for i = 1, #bodies do
      local b = bodies[i]
      b.x = b.x + dt * b.vx
      b.y = b.y + dt * b.vy
      b.z = b.z + dt * b.vz
    end
  end
end
advance(300, 0.01)
for i = 1, #bodies do
  local b = bodies[i]
  print(string.format("%.9f %.9f %.9f %.9f %.9f %.9f", b.x, b.y, b.z, b.vx, b.vy, b.vz))
end
"#,
    },
    Prog {
        name: "s4_extra_keys",
        small: false,
        src: r#"
-- the same eight core fields, different extra keys (one more node in use, other positions) and other insertion orders
local objs = {}
for i = 1, 12 do
  local o
  if i % 4 == 0 then o = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
  elseif i % 4 == 1 then o = { h = 8, g = 7, f = 6, e = 5, d = 4, c = 3, b = 2, a = 1 }
  elseif i % 4 == 2 then o = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, extra = i }
  else o = { extra = i, a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 }; o.h = 8; o["z" .. i] = i end
  objs[i] = o
end
local function sum(n)
  local s = 0
  for k = 1, n do
    local o = objs[(k - 1) % #objs + 1]
    s = s + o.a + o.b * 2 + o.c * 3 + o.d * 4 + o.e * 5 + o.f * 6 + o.g * 7 + o.h * 8
    o.a = o.a + 1
    o.h = o.h - 1
  end
  return s
end
print(sum(120), sum(600), sum(1200))
for i = 1, #objs do local o = objs[i]; print(i, o.a, o.h, o.extra) end
"#,
    },
    Prog {
        name: "s4_nil_readd",
        small: false,
        src: r#"
-- a field set to nil (the key stays in its node) and added again; the table has nine fields
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9 }
local function run(n)
  local s = 0
  for k = 1, n do
    s = s + t.a + (t.e or 7) + t.i
    if k % 7 == 0 then t.e = nil end
    if k % 7 == 3 then t.e = k end
    if k % 11 == 0 then t.a = nil; t.a = k end
    s = s + (t.e or 100)
  end
  return s
end
print(run(100), run(500), run(2000))
print(t.a, t.e, t.i)
"#,
    },
    Prog {
        name: "s4_insert_midloop",
        small: false,
        src: r#"
-- keys added in the middle of the loop (rehash): the layout changes under a trace that reads by field
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7 }
local function run(n, base)
  local s = 0
  for k = 1, n do
    s = s + t.a + t.d + t.g
    if k % 5 == 0 then t["k" .. (base + k)] = k end
    s = s + t.b + t.f
  end
  return s
end
print(run(50, 0), run(100, 1000), run(400, 2000))
local c = 0
for _ in pairs(t) do c = c + 1 end
print(c, t.a, t.g, t.k1005, t.k2400)
"#,
    },
    Prog {
        name: "s4_insert_every_iter",
        small: false,
        src: r#"
-- a new key in every iteration: every layout is new (the shape guard must demote, not storm)
local function run(n)
  local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
  local s = 0
  for k = 1, n do
    s = s + t.a + t.h + t.d
    t["n" .. k] = k
    s = s + t.b
  end
  local c = 0
  for _ in pairs(t) do c = c + 1 end
  return s, c
end
print(run(100))
print(run(1500))
"#,
    },
    Prog {
        name: "s4_fresh_tables",
        small: false,
        src: r#"
-- thirty tables of one layout whose ids are not computed when the trace first sees them: only the handler's fills leave the trace
local objs = {}
for i = 1, 30 do objs[i] = { a = i, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 } end
local function run(n)
  local s = 0
  for k = 1, n do
    local o = objs[k % 30 + 1]
    s = s + o.a + o.h
  end
  return s
end
print(run(600))
print(run(900))
"#,
    },
    Prog {
        name: "s4_many_instances",
        small: false,
        src: r#"
-- fifty instances with differing extra keys read through the same code
local inst = {}
for i = 1, 50 do
  local o = { x = i, y = i * 2, z = i * 3, w = 1, v = 2, u = 3, q = 4 }
  for k = 1, i % 7 do o["e" .. k .. "_" .. i % 5] = k end
  inst[i] = o
end
local function run(n)
  local s = 0
  for k = 1, n do
    local o = inst[k % 50 + 1]
    s = s + o.x + o.y + o.z + o.w + o.v + o.u + o.q
    o.x = o.x + 1
  end
  return s
end
print(run(500), run(5000))
"#,
    },
    Prog {
        name: "s4_gc_deadkey",
        small: false,
        src: r#"
-- keys removed, the table collected and new keys inserted into recycled nodes: the id computed for the old layout must not survive
collectgarbage("stop")
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9, j = 10 }
local function run(n, tag)
  local s = 0
  for k = 1, n do
    s = s + t.a + (t.c or 1000) + (t.h or 2000)
    if k == 20 then t.c = nil; t.h = nil; t.j = nil end
    if k == 40 then collectgarbage("restart"); collectgarbage(); collectgarbage("stop") end
    if k == 60 then t["n" .. tag] = 1; t.c = 33; t.h = 88 end
  end
  return s
end
print(run(80, 1))
print(run(80, 2))
collectgarbage("restart")
local n = 0
for k, v in pairs(t) do n = n + v end
print(n, t.c, t.h, t.j)
"#,
    },
    Prog {
        name: "s4_weak",
        small: false,
        src: r#"
-- weak tables: the entries cleared by the collector change a table's key layout; the fields are read by name in a loop
local w = setmetatable({}, { __mode = "k" })
local named = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
local function run(n)
  local s = 0
  for k = 1, n do
    local key = {}
    w[key] = k
    s = s + named.a + named.h + named.d
    if k % 16 == 0 then collectgarbage() end
  end
  return s
end
print(run(200), run(400))
local c = 0
for _ in pairs(w) do c = c + 1 end
collectgarbage()
c = 0
for _ in pairs(w) do c = c + 1 end
print(c)
local function mkw()
  return setmetatable({ a = {}, b = {}, c = {}, d = {}, e = {}, f = {}, g = {}, h = {}, i = 1 }, { __mode = "v" })
end
local wv = mkw()
collectgarbage()
-- the entries a..h are cleared now (nil values, their keys are dead and the layout changes at the next collection); i stays
local function rv(n)
  local s = 0
  for k = 1, n do
    s = s + (wv.i or 0)
    s = s + (wv.a and 1 or 2) + (wv.h and 3 or 4)
  end
  return s
end
print(rv(60))
collectgarbage()
wv.a = 5
print(rv(60))
wv.h = 9
wv.z1, wv.z2, wv.z3 = 1, 2, 3
print(rv(60), wv.z3)
"#,
    },
    Prog {
        name: "s4_sizes",
        small: false,
        src: r#"
-- tables with 7, 8, 9, 16 and 17 fields (the id slot exists from eight nodes on)
local function build(n)
  local t = {}
  for i = 1, n do t["f" .. i] = i end
  return t
end
local ts = { build(7), build(8), build(9), build(16), build(17) }
local function run(t, n)
  local s = 0
  for k = 1, n do
    s = s + t.f1 + t.f7
    t.f1 = t.f1 + 1
  end
  return s
end
for r = 1, 3 do
  for i = 1, #ts do print(i, run(ts[i], 200)) end
end
local function run8(t, n)
  local s = 0
  for k = 1, n do
    s = s + (t.f8 or 1000) + (t.f9 or 2000)
  end
  return s
end
for i = 1, #ts do print(i, run8(ts[i], 100)) end
"#,
    },
    Prog {
        name: "s4_shrink",
        small: false,
        src: r#"
-- a table that shrinks after a rehash triggered by an insert (many keys deleted, then one inserted)
local function mk()
  local t = {}
  for i = 1, 40 do t["f" .. i] = i end
  return t
end
local function run(t, n)
  local s = 0
  for k = 1, n do s = s + t.f1 + t.f2 + t.f3 end
  return s
end
local t = mk()
print(run(t, 200))
for i = 4, 40 do t["f" .. i] = nil end
print(run(t, 200))
t.g1 = 1; t.g2 = 2; t.g3 = 3; t.g4 = 4; t.g5 = 5
print(run(t, 200), t.f1, t.g5)
for i = 1, 20 do t["h" .. i] = i end
print(run(t, 200))
"#,
    },
    Prog {
        name: "s4_meta",
        small: false,
        src: r#"
-- class tables with many methods as the shaped table (__index chain), instances of two layouts
local Base = {}
Base.__index = Base
function Base.m1(s) return s.v + 1 end
function Base.m2(s) return s.v + 2 end
function Base.m3(s) return s.v + 3 end
function Base.m4(s) return s.v + 4 end
function Base.m5(s) return s.v + 5 end
function Base.m6(s) return s.v + 6 end
function Base.m7(s) return s.v + 7 end
function Base.m8(s) return s.v + 8 end
local Derived = setmetatable({}, Base)
Derived.__index = Derived
function Derived.d1(s) return s.v * 2 end
function Derived.m3(s) return s.v * 3 end
local function new(cls, v, extra)
  local o = setmetatable({ v = v }, cls)
  if extra then o.e1, o.e2, o.e3, o.e4, o.e5, o.e6, o.e7 = 1, 2, 3, 4, 5, 6, 7 end
  return o
end
local objs = { new(Base, 1), new(Derived, 2), new(Base, 3, true), new(Derived, 4, true) }
local function run(n)
  local s = 0
  for k = 1, n do
    local o = objs[k % 4 + 1]
    s = s + o:m1() + o:m3() + o:m8() + (o.d1 and o:d1() or 0)
  end
  return s
end
print(run(100), run(1000))
Derived.m1 = function(s) return -1 end
print(run(100))
Base.m8 = nil
print(pcall(run, 100))
"#,
    },
    Prog {
        name: "s4_two_keys",
        small: false,
        src: r#"
-- eight-field tables of different insertion orders read through two keys only (`shapemin`: too few covered keys for a guard, the HREFKs compare their keys)
local A = { p = 1, q = 2, r = 3, s = 4, t = 5, u = 6, v = 7, w = 8 }
local B = { w = 8, v = 7, u = 6, t = 5, s = 4, r = 3, q = 2, p = 1 }
local C = { q = 20, p = 10, s = 40, r = 30, u = 60, t = 50, w = 80, v = 70 }
local list = { A, B, C, B, A, C }
local function run(n, step)
  local s = 0
  for k = 1, n do
    local o = list[(k // step) % 6 + 1]
    s = s + o.p * 3 + o.w
  end
  return s
end
print(run(100, 1), run(1000, 1))
print(run(100, 50), run(1000, 7))
"#,
    },
    Prog {
        name: "s4_alternate",
        small: false,
        src: r#"
-- instances of three layouts flow through one trace in alternating and in blocked order
local A = { p = 1, q = 2, r = 3, s = 4, t = 5, u = 6, v = 7, w = 8 }
local B = { w = 8, v = 7, u = 6, t = 5, s = 4, r = 3, q = 2, p = 1 }
local C = { p = 1, q = 2, r = 3, s = 4, t = 5, u = 6, v = 7, w = 8, extra = 9 }
local list = { A, B, C, A, C, B }
local function run(n, step)
  local s = 0
  for k = 1, n do
    local o = list[(k // step) % 6 + 1]
    s = s + o.p + o.q * 2 + o.r * 3 + o.w * 4
    o.s = o.s + 1
  end
  return s
end
print(run(100, 1), run(1000, 1))
print(run(100, 50), run(1000, 7))
print(A.s, B.s, C.s)
"#,
    },
    Prog {
        name: "s4_late_id",
        small: false,
        src: r#"
-- tables built after the trace was recorded: their id was never computed when the guard first sees them
local function mk(i)
  return { a = i, b = i + 1, c = i + 2, d = i + 3, e = i + 4, f = i + 5, g = i + 6, h = i + 7 }
end
local keep = { mk(1) }
local function run(t, n)
  local s = 0
  for k = 1, n do s = s + t.a + t.d + t.h end
  return s
end
local total = 0
for i = 1, 40 do total = total + run(keep[1], 50) end
for i = 2, 80 do keep[i] = mk(i); total = total + run(keep[i], 30) end
print(total)
for i = 1, 80, 9 do print(i, run(keep[i], 10)) end
"#,
    },
    Prog {
        name: "s4_next_pairs",
        small: false,
        src: r#"
-- pairs / next with assignments to existing fields and insertions between traversals
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9 }
local function run(n)
  local s = 0
  for k = 1, n do
    for key, v in pairs(t) do
      s = s + v
      t[key] = v + 1
    end
    s = s + t.a + t.i
    if k % 10 == 0 then t["x" .. k] = k end
  end
  return s
end
print(run(30), run(60))
local c = 0
for _ in pairs(t) do c = c + 1 end
print(c, t.a, t.i)
"#,
    },
    Prog {
        name: "s4_tableops",
        small: false,
        src: r#"
-- table.insert / remove / move on tables that also have nine hash fields
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9 }
local function run(n)
  local s = 0
  for k = 1, n do
    table.insert(t, k)
    if k % 3 == 0 then table.remove(t) end
    if k % 5 == 0 then table.move(t, 1, #t, 2) end
    s = s + t.a + t.e + t.i + #t
    t.a = t.a + 1
  end
  return s
end
print(run(50), run(150))
print(#t, t.a, t.i)
"#,
    },
    Prog {
        name: "s4_coro",
        small: false,
        src: r#"
-- coroutines yielding between field reads; another coroutine changes the table's layout while the first is suspended
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
local co = coroutine.wrap(function()
  local s = 0
  for k = 1, 400 do
    s = s + t.a + t.h
    coroutine.yield(s)
    s = s + t.c + t.d
  end
  return s
end)
local last
for k = 1, 200 do
  last = co()
  if k == 50 then t.zz = 1; t.a = 10 end
  if k == 100 then t.c = nil; t.c = 5 end
  if k == 150 then for j = 1, 20 do t["w" .. j] = j end end
end
print(last, t.zz, t.c)
"#,
    },
    Prog {
        name: "s4_hooks",
        small: false,
        src: r#"
-- a count hook that changes the table's layout while the loop reads it by field
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
local calls = 0
local function run(n)
  local s = 0
  for k = 1, n do s = s + t.a + t.h + t.d end
  return s
end
print(run(100))
debug.sethook(function()
  calls = calls + 1
  if calls % 5 == 0 then t["hk" .. calls] = calls end
  if calls % 7 == 0 then t.d = nil; t.d = calls end
end, "", 97)
print(run(2000))
debug.sethook()
print(calls > 0, t.d ~= nil)
"#,
    },
    Prog {
        name: "s4_errors",
        small: false,
        src: r#"
-- errors thrown in the middle of a loop that mutates and reads the table by field
local t = { a = 1, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8 }
local function run(n)
  local s = 0
  for k = 1, n do
    s = s + t.a + t.h
    if k % 13 == 0 then t["e" .. k] = k end
    if k % 17 == 0 then error("boom " .. k) end
    s = s + t.d
  end
  return s
end
for i = 1, 6 do print(pcall(run, 40)) end
print(t.a, t.h, t.e13, t.e26)
"#,
    },
    Prog {
        name: "s4_strkeys",
        small: false,
        src: r#"
-- short strings used as field names are freed and created again (their addresses are reused) while a guarded trace is live
local function mk(name)
  local t = {}
  t[name] = 1
  t.a, t.b, t.c, t.d, t.e, t.f, t.g = 1, 2, 3, 4, 5, 6, 7
  return t
end
local function run(t, n)
  local s = 0
  for k = 1, n do s = s + t.a + t.g end
  return s
end
local total = 0
local live = mk("key0")
for i = 1, 300 do
  local tmp = mk("fld" .. i)
  total = total + run(tmp, 20) + run(live, 20)
  if i % 25 == 0 then collectgarbage() end
end
print(total)
local t2 = {}
for i = 1, 8 do t2["s" .. i] = i end
local function rd(t, n)
  local s = 0
  for k = 1, n do s = s + (t.s1 or 0) + (t.s8 or 0) end
  return s
end
for i = 1, 100 do
  t2["tmp" .. i] = i
  t2["tmp" .. i] = nil
  if i % 10 == 0 then collectgarbage() end
  total = total + rd(t2, 10)
end
print(total)
"#,
    },
];
