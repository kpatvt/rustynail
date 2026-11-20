-- H7: recursion with varargs, closures and upvalues
local count = 0
local function va(n, ...)
  count = count + 1
  if n == 0 then return select("#", ...), ... end
  return va(n - 1, n, ...)
end
print(va(12))
print(count)
local function mk(n)
  local acc = 0
  local function go(k)
    acc = acc + k
    if k == 0 then return function() return acc end end
    return go(k - 1)
  end
  return go(n)
end
local fs = {}
for i = 1, 20 do fs[i] = mk(i * 5) end
local t = 0
for i = 1, 20 do t = t + fs[i]() end
print(t)
local function sum(t, i) i = i or 1 if i > #t then return 0 end return t[i] + sum(t, i + 1) end
local arr = {} for i = 1, 500 do arr[i] = i end
print(sum(arr), sum(arr), sum(arr))
local function depth(n, f) if n == 0 then return f() end return depth(n - 1, function() return f() + 1 end) end
print(depth(200, function() return 0 end))
