-- P1 (`presize`, docs/JIT_TRACE.md section 32): a trace inserts a group of constant keys into a table it allocated through one presizing
-- insertion and inline plans. The layout must stay the interpreter's (and the reference's): every table below is printed in `pairs` order
-- (seed-0 builds: the string hashes, hence the collisions, are the reference's). Groups of 2..8 keys with several name sets (for three or
-- more keys some sets give the growth's layout when presized and some do not: the compiler must presize only the first kind), the
-- object idiom, a guard inside the group (an exit there must not leave a presized table behind: an integer key inserted later would go to
-- the hash part instead of the array part), metatables changed between the stores, a table that is its own metatable, nil and dynamic keys
-- inside a group, growth past the presized part, and integer keys after the group.
local function order(t)
  local o = {}
  for k, v in pairs(t) do o[#o + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(o, " ") .. " #" .. #t
end

-- 1. groups of 2..8 keys, each in its own hot loop
local function g2(n) local t for i = 1, n do t = {} t.alpha = i t.beta = -i end return t end
local function g2b(n) local t for i = 1, n do t = {} t.k7 = i t.k8 = i + 1 end return t end
local function g2c(n) local t for i = 1, n do t = {} t.kx = i t.ky = i + 1 end return t end
local function g3a(n) local t for i = 1, n do t = {} t.k1 = i t.k2 = i + 1 t.k3 = i + 2 end return t end
local function g3b(n) local t for i = 1, n do t = {} t.a3 = i t.b3 = i + 1 t.c3 = i + 2 end return t end
local function g3c(n) local t for i = 1, n do t = {} t.p = i t.q = i + 1 t.r = i + 2 end return t end
local function g3d(n) local t for i = 1, n do t = {} t.name = i t.kind = i + 1 t.size = i + 2 end return t end
local function g4a(n) local t for i = 1, n do t = {} t.w = i t.x = i + 1 t.y = i + 2 t.z = i + 3 end return t end
local function g4b(n) local t for i = 1, n do t = {} t.u1 = i t.u2 = i + 1 t.u3 = i + 2 t.u4 = i + 3 end return t end
local function g4c(n) local t for i = 1, n do t = {} t.left = i t.right = i + 1 t.top = i + 2 t.bottom = i + 3 end return t end
local function g5(n) local t for i = 1, n do t = {} t.e1 = i t.e2 = i t.e3 = i t.e4 = i t.e5 = i end return t end
local function g6(n) local t for i = 1, n do t = {} t.f1 = i t.f2 = i t.f3 = i t.f4 = i t.f5 = i t.f6 = i end return t end
local function g7(n) local t for i = 1, n do t = {} t.s1 = i t.s2 = i t.s3 = i t.s4 = i t.s5 = i t.s6 = i t.s7 = i end return t end
local function g8(n)
  local t
  for i = 1, n do t = {} t.h1 = i t.h2 = i t.h3 = i t.h4 = i t.h5 = i t.h6 = i t.h7 = i t.h8 = i end
  return t
end
local function g9(n)
  local t
  for i = 1, n do t = {} t.m1 = i t.m2 = i t.m3 = i t.m4 = i t.m5 = i t.m6 = i t.m7 = i t.m8 = i t.m9 = i end
  return t
end
local groups = { {"g2", g2}, {"g2b", g2b}, {"g2c", g2c}, {"g3a", g3a}, {"g3b", g3b}, {"g3c", g3c}, {"g3d", g3d}, {"g4a", g4a},
  {"g4b", g4b}, {"g4c", g4c}, {"g5", g5}, {"g6", g6}, {"g7", g7}, {"g8", g8}, {"g9", g9} }
for _, g in ipairs(groups) do
  local t = g[2](300)
  print(g[1], order(t))
  -- growth past the presized part, then integer keys (they go to the array part only when the hash part is full)
  t.extra1 = 1; t[1] = 1; t.extra2 = 2; t[2] = 2
  print(g[1] .. "+", order(t))
end

-- 2. the object idiom: setmetatable({}, C) in a constructor, the fields set by an init method
local Point = {}
Point.__index = Point
function Point.new(x, y) local self = setmetatable({}, Point) self:init(x, y) return self end
function Point:init(x, y) self.x = x or 0 self.y = y or 0 end
local Rect = {}
Rect.__index = Rect
function Rect.new(a, b, c) local self = setmetatable({}, Rect) self.width = a self.height = b self.color = c return self end
local function objs(n)
  local s, last, lr = 0, nil, nil
  for i = 1, n do
    last = Point.new(i, -i)
    lr = Rect.new(i, i * 2, "c" .. (i % 3))
    s = s + last.x + last.y + lr.width
  end
  return s, last, lr
end
local s, p, r = objs(400)
print("objs", s, order(p), order(r))
p[1] = 1; r.extra = true
print("objs+", order(p), order(r))

-- 3. a guard inside the group: some tables leave the function after the first key; an integer key is added to those afterwards
local function w1(i, fl) local t = {} t.wa = i if fl[i] then return t end t.wb = i return t end
local function w2(i, fl) local t = {} t.wc = i if fl[i] then return t end t.wd = i return t end
local function w3(i, fl) local t = {} t.we = i if fl[i] then return t end t.wf = i return t end
local function w4(i, fl) local t = {} t.wg = i if fl[i] then return t end t.wh = i return t end
-- (the flag is loaded inside the group, so its type guard sits between the two insertions; the table escapes: it is not sunk)
local keep = {}
local function window(f, n, every)
  local out, fl = {}, {}
  for i = 1, n do fl[i] = i % every == 0 end
  for i = 1, n do
    local t = f(i, fl)
    keep[1] = t
    if fl[i] then t[1] = i out[#out + 1] = order(t) end
  end
  return table.concat(out, " | ")
end
print("window1", window(w1, 300, 37))
print("window2", window(w2, 300, 41))
print("window3", window(w3, 300, 43))
print("window4", window(w4, 300, 47))

-- 4. the metatable changes between two stores (a `__newindex` appears for some objects)
local Plain = {}
local Times10 = { __newindex = function(t, k, v) rawset(t, k, v * 10) end }
local mts = { Plain, Plain, Plain, Times10 }
local function meta(n)
  local out = 0
  for i = 1, n do
    local t = setmetatable({}, Plain)
    t.ma = i
    setmetatable(t, mts[i % 4 + 1])
    t.mb = i
    out = out + rawget(t, "mb")
  end
  return out
end
print("meta", meta(400))

-- 5. a table that becomes its own metatable at run time, then gets `__newindex` by the first store of the group
local mtreg = { {}, false }
local function NI(t, k, v) rawset(t, k, v * 10) end
local function own(n, alias_every)
  local out = 0
  for i = 1, n do
    local t = {}
    mtreg[2] = t
    -- (the index without a branch: 2 every `alias_every`-th time, else 1; a branch would be a guard that sends the aliased case to the interpreter)
    setmetatable(t, mtreg[1 + i // alias_every - (i - 1) // alias_every])
    t.__newindex = NI
    t.oy = i
    out = out + rawget(t, "oy")
  end
  return out
end
print("own", own(400, 7))

-- 6. a nil value and a dynamic key inside a group, a key stored twice
local keys = { "d1", "d2", "d3" }
local function mixed(n)
  local t
  for i = 1, n do
    t = {}
    t.na = i
    t.nb = nil
    t.nc = i
    t[keys[i % 3 + 1]] = i
    t.nd = i
    t.na = -i
  end
  return t
end
print("mixed", order(mixed(300)))

-- 7. the constructor with a hash size (no presizing: the part exists), and an array part first
local function ctor(n)
  local a, b
  for i = 1, n do
    a = { ca = i }
    a.cb = i
    a.cc = i
    b = { i }
    b.cd = i
    b.ce = i
  end
  return a, b
end
local ca, cb = ctor(300)
print("ctor", order(ca), order(cb))
