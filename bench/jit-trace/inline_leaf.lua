-- J5 gate set (c) `inline_leaf` (docs/JIT_TRACE.md 1.6): a loop calling a 3-line local function (an upvalue of the
-- looping function), integers. N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see inline_leaf.expected
local function leaf(a, b)
  local t = a * b
  return t + a - b
end
local function run(n)
  local s = 0
  for i = 1, n do s = s + leaf(i, 3) end
  return s
end
local reps = tonumber(arg and arg[1]) or 200000
local total = 0
for r = 1, reps do total = total + run(1000) end
print(total)
