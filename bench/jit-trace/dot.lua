-- J5 informational set (b') `dot` (docs/JIT_TRACE.md 1.6; the matmul inner-loop shape, two arrays, as in
-- bench/jit-oracle/dot_w.lua but with a for loop). N = reps (arg[1]); inner n = 1000.
-- Expected stdout (default N): see dot.expected
local function dot(a, b, n)
  local s = 0.0
  for i = 1, n do s = s + a[i] * b[i] end
  return s
end
local n = 1000
local a, b = {}, {}
for i = 1, n do
  a[i] = (i % 7) / 4.0
  b[i] = (i % 5) / 8.0 + 0.5
end
local reps = tonumber(arg and arg[1]) or 400000
local total = 0.0
for r = 1, reps do total = total + dot(a, b, n) end
print(string.format("%.6f", total))
