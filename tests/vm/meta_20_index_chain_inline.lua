-- __index chains of tables followed inline by GETFIELD/SELF/GETTABUP (vm_unsafe
-- `index_chain_short_str`): depth limits, functions in the chain, flag-cached absence,
-- metatables changed between lookups, nil results.
local function chain(n)
  local top = {}
  local t = top
  for i = 1, n do
    local nxt = {["k" .. i] = i}
    setmetatable(t, {__index = nxt})
    t = nxt
  end
  return top, t
end

-- every depth around the inline limit (4)
for n = 1, 7 do
  local top, last = chain(n)
  last.m = function(self, x) return n * 100 + x end
  local r = {}
  for i = 1, n do r[#r + 1] = top["k" .. i] end
  print(n, table.concat(r, ","), top.missing, top:m(n), top.k1)
end

-- a function at the end (or in the middle) of the chain
local f = setmetatable({}, {__index = function(t, k) return "fn:" .. k end})
local mid = setmetatable({a = 1}, {__index = f})
local o = setmetatable({}, {__index = mid})
print(o.a, o.b, o.zz)
local deep = setmetatable({}, {__index = setmetatable({}, {__index = setmetatable({}, {__index = o})})})
print(deep.a, deep.b, deep.c)

-- metatable without __index (absence then cached in the flags), then __index added
local mt = {}
local p = setmetatable({}, mt)
print(p.x, p.x, p.y)
mt.__index = {x = "now"}
print(p.x, p.y)
mt.__index = nil
print(p.x)
mt.__index = function(_, k) return k .. "!" end
print(p.x)

-- methods through classes; the class changes between calls
local A = {}; A.__index = A
function A:who() return "A" end
local B = setmetatable({}, {__index = A}); B.__index = B
local b = setmetatable({}, B)
print(b:who())
function B:who() return "B" end
print(b:who())
B.who = nil
print(b:who())
getmetatable(B).__index = {who = function() return "C" end}
print(b:who())

-- globals with a metatable on _ENV
local env = setmetatable({}, {__index = setmetatable({}, {__index = _G})})
local fn = load("return print ~= nil, nope, string.format('%d', 7)", "x", "t", env)
print(fn())

-- loops: the lookup inside a loop sees updates
local base = {v = 0}
local obj = setmetatable({}, {__index = setmetatable({}, {__index = base})})
local s = 0
for i = 1, 10 do base.v = i; s = s + obj.v end
print(s)

-- non-table __index in the chain (string) and an error through the chain
local str = setmetatable({}, {__index = "abc"})
print(str.len)
local bad = setmetatable({}, {__index = setmetatable({}, {__index = 42})})
print(pcall(function() return bad.x end))
-- a loop in the chain
local l1, l2 = {}, {}
setmetatable(l1, {__index = l2}); setmetatable(l2, {__index = l1})
print(pcall(function() return l1.x end))
