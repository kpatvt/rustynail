-- F4: expression temporaries live across guards whose exit re-enters the interpreter mid-expression
local function f(t, n)
  local s = 0.0
  for i = 1, n do
    s = s + (t[i] * 2.5 + t[i + 1] * 1.5 - t[i + 2] / 3) * (t[i] + 0.25) + (t[i + 3] or 0) * 0.125
  end
  return s
end
local data = {}
for i = 1, 400 do data[i] = (i % 7) * 0.5 + 1 end
local r = 0.0
for k = 1, 30 do r = r + f(data, 380) end
print(string.format("%.9g", r))
-- types change in the middle: an integer, a numeric string, nil (error), a table with __add
data[150] = 7
data[200] = "3.5"
local mt = setmetatable({}, { __add = function(a, b) return 1.25 end, __mul = function(a, b) return 2.5 end,
  __sub = function() return 0.5 end, __div = function() return 0.75 end })
print(string.format("%.9g", f(data, 380)))
data[250] = mt
print(string.format("%.9g", f(data, 380)))
data[250] = 2.5
data[300] = nil
local ok, e = pcall(f, data, 380)
print(ok, e)
data[300] = 1.0
print(string.format("%.9g", f(data, 380)))
-- integer / float mixed arithmetic with a conversion in the middle of an expression
local function g(n, x)
  local a, s = 0, 0
  for i = 1, n do
    a = a + i
    s = s + (a * 3 + x * i - (a // 2) + i % 3) * (x + a)
  end
  return s
end
for k = 1, 20 do g(50, 0.5) end
print(g(100, 0.5), g(100, 2), g(100, 1 << 40), math.type(g(10, 1)))
