-- T6: intrinsic call sites (math builtins with typed operands), Lua callees with typed results, callee replaced mid-loop
local sqrt, floor, ceil, abs, min, max = math.sqrt, math.floor, math.ceil, math.abs, math.min, math.max
local sin, exp = math.sin, math.exp
local function sq(x) return x * 0.5 end
local function f(n)
  local s, x, k, t = 0.0, 0.5, 3, 0
  for i = 1, n do
    x = x * 1.37 + 0.11
    if x > 1e6 then x = x - 1e6 end
    s = s + sqrt(x) + abs(x) - min(x, 2.25) * max(x, 3.5) + sin(x) + exp(x * 1e-6)
    t = t + floor(x) + ceil(x) + abs(k - i) + min(i, k) + max(i, k) + floor(i)
    s = s + sq(i) * 2
    if i == 40 then sqrt = function(v) return v end end
    if i == 70 then sqrt = math.sqrt; k = k + 0.5 end
  end
  return s, t
end
print(f(100))
print(f(100))
local co = coroutine.wrap(function(n)
  local a = 0.0
  for i = 1, n do a = a + sqrt(i) + floor(a); if i % 5 == 0 then coroutine.yield(a) end end
  return -a
end)
local v = co(40)
while v >= 0 do v = co() end
print(v)
