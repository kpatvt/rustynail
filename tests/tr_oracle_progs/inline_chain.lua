-- three levels of inlined callees; the innermost one leaves its temporaries behind and a rare branch exits the
-- trace from inside the callee frames (TR3: popped and live callee frames in the restored state)
local function leaf(x, k)
  local t = x * 2
  local u = t + k
  if u % 97 == 0 then u = u - 1 end
  return u
end
local function mid(x, k)
  local a = leaf(x, k)
  local b = leaf(a, k + 1)
  return a + b
end
local function top(x, k)
  local r = mid(x, k)
  return r - mid(x + 1, k)
end
local function run(n, k)
  local s = 0
  for i = 1, n do s = s + top(i, k) end
  return s
end
local t = 0
for rep = 1, 12 do t = t + run(700 + rep, rep) end
print(t)
