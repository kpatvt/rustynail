-- J5 gate set (c) `inline_2` (docs/JIT_TRACE.md 1.6): two levels of inlining (loop -> mid -> leaf), floats.
-- N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see inline_2.expected
local function leaf(a, b)
  local t = a * b
  return t + a - b
end
local function mid(x)
  local y = leaf(x, 0.5)
  return y * 0.25 + 1.0
end
local function run(n)
  local s = 0.0
  for i = 1, n do s = s + mid(i + 0.0) end
  return s
end
local reps = tonumber(arg and arg[1]) or 80000
local total = 0.0
for r = 1, reps do total = total + run(1000) end
print(string.format("%.6f", total))
