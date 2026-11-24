-- docs/JIT_TRACE.md section 44.1 (T6, `ctor`): the fused table constructor of tier 1 (NEWTABLE + constant-key SETFIELDs whose main positions are known when the
-- function is compiled). Same output as the reference in every mode, including the iteration order of the created tables (their node layout).

local function dump(t)
  local ks = {}
  for k, v in pairs(t) do ks[#ks + 1] = tostring(k) .. "=" .. tostring(v) end
  return table.concat(ks, ",")
end

-- (1) the record constructors of the benchmarks: registers and constants as values
local V = {}
V.__index = V
local function vec(x, y, z) return setmetatable({ x = x, y = y, z = z }, V) end
local function rec(a, b) return { name = "n", count = a, ratio = b, flag = true, none = false } end
local function pt(x, y) return { x = x, y = y } end
local function one(x) return { only = x } end
local function mixed(x) return { x = x, 10, 20, y = x, 30 } end
local function big(i) return { a = i, b = 2, c = 3, d = 4, e = 5, f = 6, g = 7, h = 8, i = 9, j = 10, k = 11, l = 12, m = 13, n = 14, o = 15, p = 16 } end
for r = 1, 3 do
  local v = vec(r, r + 0.5, "s" .. r)
  print("vec", dump(v), getmetatable(v) == V)
  print("rec", dump(rec(r, r / 3)))
  print("pt", dump(pt(r, -r)))
  print("one", dump(one({})) ~= nil, next(one(r)))
  print("mixed", dump(mixed(r)), #mixed(r))
  print("big", dump(big(r)))
end

-- (2) nil values are no-ops that must not create the key; the later keys still land where the interpreter puts them
local function nilv(a, b) return { p = a, q = b, r = 3, s = a } end
local function nilk() return { p = nil, q = 2, r = nil, s = 4 } end
print("nilv", dump(nilv(1, nil)), dump(nilv(nil, 2)), dump(nilv(nil, nil)), dump(nilv(1, 2)))
print("nilk", dump(nilk()))
local t = nilv(nil, nil)
t.p = 5
t.q = 6
print("nilv grown", dump(t))

-- (3) a constructor of a metatable: the keys may be metamethod names (the cached "absent" flags must not survive)
local function mt_index(base) return { __index = base } end
local function mt_full(f) return { __index = f, __newindex = function(t, k, v) rawset(t, k, v * 2) end, __call = function(_, a) return a + 1 end, __len = function() return 42 end } end
local base = { hello = "world" }
local o = setmetatable({}, mt_index(base))
print("mt_index", o.hello, o.nothing)
local o2 = setmetatable({}, mt_full(function(t, k) return "idx:" .. k end))
o2.v = 21
print("mt_full", o2.v, o2.zzz, o2(1), #o2)
local function mt_eq() return { __eq = function() return true end, __lt = function() return true end, __concat = function() return "cat" end, __tostring = function() return "TS" end } end
local ea, eb = setmetatable({}, mt_eq()), setmetatable({}, mt_eq())
print("mt_eq", ea == eb, ea < eb, ea .. "x", tostring(ea))

-- (4) many different key sets: main-position collisions in tables of 1, 2, 4, 8 and 16 nodes, in tier 1 (called a few times, straight-line) and in loops
local function name(i, j) return "k" .. i .. "_" .. j * 7 end
local srcs = {}
for i = 1, 60 do
  local n = 1 + (i % 9)
  local parts = {}
  for j = 1, n do parts[#parts + 1] = name(i, j) .. " = " .. (j % 3 == 0 and "a" or (j % 3 == 1 and "b" or tostring(j))) end
  srcs[#srcs + 1] = "return function(a, b) return {" .. table.concat(parts, ", ") .. "} end"
end
for i = 1, #srcs do
  local f = assert(load(srcs[i]))()
  local t1, t2, t3 = f(i, "s"), f(1.5, {}), f(nil, i)
  print("set", i, dump(t1), dump(t2) ~= nil, dump(t3))
  local last
  for r = 1, 5 do last = f(r, r) end
  print("setloop", i, dump(last))
end

-- (4b) a store into another table right behind the constructor (the group ends at it: another register)
local sibsrc = {}
for i = 1, 90 do
  sibsrc[#sibsrc + 1] = "return function(a, u) local t = {s" .. i .. "_a = a, s" .. i .. "_b = 2, s" .. i .. "_c = 3, s" .. i .. "_d = a, s" .. i .. "_e = 5}; u.s" .. i .. "_z = a; return t end"
end
for i = 1, #sibsrc do
  local f = assert(load(sibsrc[i]))()
  local u = {}
  local t1 = f(i, u)
  local t2 = f(i + 0.5, u)
  print("sib", i, dump(t1), dump(t2), dump(u))
end

-- (5) the tables survive collections and are mutated afterwards (barriers on the values stored later)
local keep = {}
for i = 1, 200 do
  local v = vec(i, i, i)
  v.w = { i }
  keep[i % 17 + 1] = v
  if i % 40 == 0 then collectgarbage() end
end
local s = 0
for _, v in pairs(keep) do s = s + v.x + v.w[1] end
print("keep", s)

-- (6) an error between the constructor and a later store, and a table that is its own value holder
local function boom(a) return { a = a, b = a.x, c = 3 } end
print("boom", (pcall(boom, { x = 1 })))
print("boom", pcall(boom, 5))
local function selfref() local t = {}; t.me = t; return t end
print("selfref", selfref().me ~= nil)
