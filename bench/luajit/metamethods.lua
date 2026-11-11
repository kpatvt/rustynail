local floor = math.floor
local n = tonumber(arg and arg[1]) or 8000000
local V = {}
V.__index = V
V.__add = function(a, b) return setmetatable({x = a.x + b.x, y = a.y + b.y}, V) end
V.__eq = function(a, b) return a.x == b.x and a.y == b.y end
V.__lt = function(a, b) return a.x < b.x end
V.__call = function(self, k) return self.x * k end
-- LuaJIT (5.1 semantics) ignores __len on tables; __unm exercises the same unary metamethod path
V.__unm = function(self) return self.x + self.y end
V.__concat = function(a, b) return "v" end
local function new(x, y) return setmetatable({x = x, y = y}, V) end
local base = setmetatable({}, {__index = function(t, k) return k * 2 end})
local chain = setmetatable({}, {__index = setmetatable({a = 1}, {__index = setmetatable({b = 2}, {__index = {c = 3}})})})
local acc = new(0, 0)
local one = new(1, 2)
local s = 0
for i = 1, floor(n / 2) do
  acc = acc + one
  if acc == one then s = s + 1 end
  if one < acc then s = s + 1 end
  s = s + acc(2) + (-acc) + base[i % 100] + chain.a + chain.b + chain.c
  local c = one .. one
end
print(string.format("%d", acc.x), string.format("%d", acc.y), string.format("%d", s))
