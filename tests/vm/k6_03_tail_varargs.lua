-- K6: tail calls and varargs inside leaf-heavy self recursion
local function sum(...)
  local n = select("#", ...)
  if n == 0 then return 0 end
  local a = ...
  if n == 1 then return a end
  return a + sum(select(2, ...))
end
local acc = 0
for i = 1, 300 do acc = acc + sum(i, 2, 3, i % 7, 5) end
print("vsum", acc)
local function count(n, acc2) if n == 0 then return acc2 end return count(n - 1, acc2 + n % 3) end
local t = 0
for i = 1, 400 do t = t + count(i % 40, 0) end
print("tc", t)
local function tf(a, b, c) if a <= 0 then return b end return tf(a - 1, c, b + 1) end
local u = 0
for i = 1, 400 do u = u + tf(i % 9, 1, 2) end
print("tf", u)
local function tup(n) if n == 0 then return 1, 2, 3 end local a, b, c = tup(n - 1) return b, c, a + n end
local r = 0
for i = 1, 300 do local a, b, c = tup(i % 8) r = r + a + b + c end
print("tup", r)
